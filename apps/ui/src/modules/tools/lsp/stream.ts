/**
 * Language servers in file windows: one channel per project
 * to its machine's daemon (a WebSocket), shared by every window of the
 * project, and the language service each file gets through it. While no
 * server answers, definitions and references are approximate: Tree-sitter
 * tags and a project search by name.
 */
import { editProjectFile, fetchProject, findWord } from "@/modules/projects";
import { projectToolsPath, restartServer } from "@/modules/tools/lsp/api";
import {
	applyEdits,
	type ExternalInfo,
	hoverMarkdown,
	type LspDiagnostic,
	plainStatus,
	pluginFixes,
	positionsOf,
	replaceEdits,
	type StatusFrame,
	statusOf,
	storedDiagnostic,
	toCompletions,
	toDiagnostics,
	toFileEdits,
	toLocations,
	toSignature,
	toSymbols,
	toTextEdits,
} from "@/modules/tools/lsp/model";
import { fetchFileProblems, onProblemsChanged } from "@/modules/tools/problems";
import type {
	CodeAction,
	Diagnostic,
	DocumentChange,
	FileEdits,
	LanguageFile,
	LanguageService,
	LanguageStatus,
	Location,
	TextRange,
} from "@/shared/editor/model";
import { externalName, nameExternal } from "@/shared/editor/registry";
import { t } from "@/shared/i18n";
import { loadSyntax } from "@/shared/syntax/load";

type Frame = { type?: string; [key: string]: unknown };

/** A daemon's answer: its result, which server gave it, how to show externals. */
interface Answer {
	result: unknown;
	server?: string;
	externals: Record<string, ExternalInfo>;
}

/** Why a request failed: `not_ready`, `no_server`, `unsupported`, `disconnected`… */
class LspError extends Error {
	constructor(
		readonly code: string,
		message: string,
	) {
		super(message);
	}
}

/** One file open on a channel. */
interface Open {
	language: string;
	text: () => string;
	onStatus: (frame: StatusFrame) => void;
	onDiagnostics: (source: string, items: LspDiagnostic[]) => void;
	onApplyEdit: (edits: FileEdits[]) => void;
}

/** How long a channel stays open after its last file closes. */
const LINGER = 30_000;

class Channel {
	private socket: WebSocket | null = null;
	private ready = false;
	private queue: string[] = [];
	private next = 1;
	private failures = 0;
	private timer: ReturnType<typeof setTimeout> | undefined;
	private pending = new Map<
		number,
		{ resolve: (answer: Answer) => void; reject: (error: LspError) => void }
	>();
	readonly files = new Map<string, Open>();
	/** Who ran a command last, for the edits it makes. */
	commander: Open | null = null;
	/** Set when the machine cannot serve language tools at all. */
	unavailable: string | null = null;
	readonly project: Promise<{ machineId: string; projectId: string } | null>;

	constructor(readonly projectId: string) {
		this.project = fetchProject(projectId)
			.then((found) => ({ machineId: found.machineId, projectId: found.id }))
			.catch(() => null);
	}

	private connect() {
		if (this.socket || typeof WebSocket === "undefined") return;
		const scheme = globalThis.location?.protocol === "https:" ? "wss:" : "ws:";
		const socket = new WebSocket(
			`${scheme}//${globalThis.location?.host}/api/v1/projects/${this.projectId}/lsp`,
		);
		this.socket = socket;
		socket.addEventListener("open", () => {
			this.ready = true;
			this.failures = 0;
			this.unavailable = null;
			for (const item of this.queue.splice(0)) socket.send(item);
		});
		socket.addEventListener("message", (message) => {
			if (typeof message.data === "string") this.receive(JSON.parse(message.data) as Frame);
		});
		socket.addEventListener("close", () => this.closed());
	}

	private closed() {
		const wasReady = this.ready;
		this.socket = null;
		this.ready = false;
		this.queue = [];
		for (const waiter of this.pending.values())
			waiter.reject(new LspError("disconnected", t("tools:lspErrors.disconnected")));
		this.pending.clear();
		if (!this.files.size) return;
		this.failures = wasReady ? 1 : this.failures + 1;
		if (this.failures >= 3) void this.explain();
		const delay = Math.min(30_000, 1000 * 2 ** Math.min(this.failures, 5));
		clearTimeout(this.timer);
		this.timer = setTimeout(() => {
			if (!this.files.size) return;
			this.connect();
			// Files open again as they are now.
			for (const [path, file] of this.files) this.sendOpen(path, file);
		}, delay);
	}

	/** Says why language tools are missing when the channel keeps failing. */
	private async explain() {
		const reason = t("tools:lspErrors.unreachable");
		this.unavailable = reason;
		for (const file of this.files.values())
			file.onStatus({ path: "", language: file.language, tools: [], configured: false });
	}

	private receive(frame: Frame) {
		switch (frame.type) {
			case "response": {
				const id = Number(frame.id);
				const waiter = this.pending.get(id);
				if (!waiter) return;
				this.pending.delete(id);
				if (frame.error) {
					const error = frame.error as { code?: string | number; message?: string };
					waiter.reject(
						new LspError(
							String(error.code ?? "failed"),
							error.message ?? t("tools:lspErrors.failed"),
						),
					);
				} else
					waiter.resolve({
						result: frame.result,
						server: typeof frame.server === "string" ? frame.server : undefined,
						externals: (frame.externals as Record<string, ExternalInfo>) ?? {},
					});
				for (const [token, info] of Object.entries(
					(frame.externals as Record<string, ExternalInfo>) ?? {},
				))
					nameExternal(`external:${token}`, info.name);
				return;
			}
			case "status":
				this.files.get(String(frame.path))?.onStatus(frame as unknown as StatusFrame);
				return;
			case "diagnostics":
				this.files
					.get(String(frame.path))
					?.onDiagnostics(String(frame.source), (frame.items as LspDiagnostic[]) ?? []);
				return;
			case "applyEdit":
				this.commander?.onApplyEdit(toFileEdits(frame.edit));
				return;
			default:
		}
	}

	private send(frame: Frame) {
		const text = JSON.stringify(frame);
		this.connect();
		if (this.ready && this.socket) this.socket.send(text);
		else this.queue.push(text);
	}

	private sendOpen(path: string, file: Open) {
		this.send({ type: "open", path, language: file.language, version: 1, text: file.text() });
	}

	open(path: string, file: Open) {
		clearTimeout(this.timer);
		this.files.set(path, file);
		this.sendOpen(path, file);
	}

	change(path: string, version: number, changes: unknown[]) {
		if (this.files.has(path)) this.send({ type: "change", path, version, changes });
	}

	save(path: string) {
		if (this.files.has(path)) this.send({ type: "save", path });
	}

	close(path: string, file: Open) {
		if (this.files.get(path) !== file) return;
		this.files.delete(path);
		this.send({ type: "close", path });
		if (!this.files.size) {
			clearTimeout(this.timer);
			this.timer = setTimeout(() => {
				if (this.files.size) return;
				this.socket?.close();
				channels.delete(this.projectId);
			}, LINGER);
		}
	}

	request(path: string, method: string, params: unknown, server?: string): Promise<Answer> {
		if (this.unavailable && !this.ready)
			return Promise.reject(new LspError("unavailable", this.unavailable));
		const id = this.next++;
		return new Promise<Answer>((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.send({ type: "request", id, path, method, params, ...(server ? { server } : {}) });
		});
	}
}

const channels = new Map<string, Channel>();

function channelOf(projectId: string): Channel {
	let channel = channels.get(projectId);
	if (!channel) {
		channel = new Channel(projectId);
		channels.set(projectId, channel);
	}
	return channel;
}

/** A file outside the project, as a language server granted it. */
export async function loadExternal(projectId: string, path: string) {
	const answer = await channelOf(projectId).request(path, "ahab/externalFile", {
		ref: path.slice("external:".length),
	});
	const found = answer.result as { text: string; info: ExternalInfo };
	nameExternal(path, found.info.name);
	return {
		text: found.text,
		name: found.info.name,
		origin: found.info.label,
		display: found.info.display,
	};
}

/** Runs `ask`; on a language server's absence, `fallback` instead. */
async function orElse<T>(ask: () => Promise<T>, fallback: () => Promise<T>): Promise<T> {
	try {
		return await ask();
	} catch (error) {
		if (error instanceof LspError) return fallback();
		throw error;
	}
}

/** The word around `offset`, and where it starts. */
function wordAt(text: string, offset: number): { word: string; from: number } {
	let from = offset;
	while (from > 0 && /[\w$]/.test(text[from - 1] ?? "")) from--;
	let to = offset;
	while (to < text.length && /[\w$]/.test(text[to] ?? "")) to++;
	return { word: text.slice(from, to), from };
}

/** What the status item's popover offers. */
function actionsOf(
	frame: StatusFrame | null,
	place: { machineId: string; projectId: string } | null,
): LanguageStatus["actions"] {
	if (!place) return [];
	const actions: LanguageStatus["actions"] = [];
	const servers = (frame?.tools ?? []).flatMap((tool) => (tool.server ? [tool.server] : []));
	if (servers.length)
		actions.push({
			label: t("tools:languageStatus.actions.restart"),
			run: () => {
				for (const server of servers) void restartServer(place.machineId, server);
			},
		});
	const machine = `/settings/machines/${place.machineId}/lsps`;
	for (const tool of frame?.tools ?? [])
		if (tool.state === "missing" && tool.installable)
			actions.push({
				label: t("tools:languageStatus.actions.install", { tool: tool.name }),
				to: machine,
			});
	if (servers.length) actions.push({ label: t("tools:languageStatus.actions.logs"), to: machine });
	actions.push({ label: t("tools:languageStatus.actions.machine"), to: machine });
	actions.push({
		label: t("tools:languageStatus.actions.project"),
		to: projectToolsPath(place.projectId),
	});
	return actions;
}

/**
 * The language service: every file of a project through its
 * channel. Files outside the project too (`external:`), read-only.
 */
export function toolsLanguageService(file: LanguageFile): LanguageService | null {
	const { document, projectId } = file;
	const path = document.path;
	const channel = channelOf(projectId);
	const statusListeners = new Set<(status: LanguageStatus | null) => void>();
	const diagnosticListeners = new Set<(items: Diagnostic[]) => void>();
	const bySource = new Map<string, LspDiagnostic[]>();
	let language: string | null = null;
	let languageName = "";
	let frame: StatusFrame | null = null;
	let disposed = false;
	// The text the daemon has: changes are sent against it.
	let sent = document.text();
	let version = 1;
	let place: { machineId: string; projectId: string } | null = null;
	const links = (shown: StatusFrame | null) => actionsOf(shown, place);

	const tell = () => {
		const shown: LanguageStatus | null = !language
			? null
			: channel.unavailable
				? {
						...plainStatus(languageName),
						label: t("tools:languageStatus.basic", { language: languageName }),
						tone: "warning",
						notes: [channel.unavailable, t("tools:languageStatus.approximate")],
						actions: links(null),
					}
				: {
						...(frame ? statusOf(frame, languageName) : plainStatus(languageName)),
						actions: links(frame),
					};
		for (const listener of statusListeners) listener(shown);
	};
	// What the project's Problems hold for this file, from tools that do
	// not report it live (checks run on save or by hand), kept current.
	const stored = new Map<string, LspDiagnostic[]>();
	const shareDiagnostics = () => {
		const items = [...bySource.entries()].flatMap(([source, list]) =>
			toDiagnostics(list, document, source),
		);
		for (const [source, list] of stored)
			if (!bySource.has(source)) items.push(...toDiagnostics(list, document, source));
		for (const listener of diagnosticListeners) listener(items);
	};
	const loadStored = () => {
		if (path.startsWith("external:")) return;
		void fetchFileProblems(projectId, path)
			.then((list) => {
				if (disposed) return;
				stored.clear();
				for (const problem of list.items)
					if (problem.path === path)
						stored.set(problem.source, [
							...(stored.get(problem.source) ?? []),
							storedDiagnostic(problem),
						]);
				shareDiagnostics();
			})
			.catch(() => {});
	};
	const stopStored = path.startsWith("external:")
		? undefined
		: onProblemsChanged(projectId, loadStored);
	loadStored();
	const applyFiles = async (edits: FileEdits[]) => {
		for (const fileEdits of edits) {
			if (fileEdits.path === path) file.edit?.(toTextEdits(fileEdits.edits, document));
			else if (!fileEdits.path.startsWith("external:"))
				// Each file is read, changed and saved over the version read.
				// oxlint-disable-next-line no-await-in-loop
				await editProjectFile(projectId, fileEdits.path, (text) =>
					applyEdits(text, fileEdits.edits),
				);
		}
	};
	const open = {
		get language() {
			return language ?? "plaintext";
		},
		text: () => sent,
		onStatus: (next: StatusFrame) => {
			frame = next.path ? next : frame;
			tell();
		},
		onDiagnostics: (source: string, items: LspDiagnostic[]) => {
			bySource.set(source, items);
			shareDiagnostics();
		},
		onApplyEdit: (edits: FileEdits[]) => void applyFiles(edits),
	};

	void loadSyntax().then((syntax) => {
		if (disposed) return;
		const found = document.languageId
			? syntax.language(document.languageId)
			: syntax.detect(externalName(path) ?? path, document.text());
		if (!found || found.id === "plaintext") {
			tell();
			return;
		}
		language = found.id;
		languageName = found.name;
		void channel.project.then((known) => {
			place = known;
			tell();
		});
		sent = document.text();
		channel.open(path, open);
		tell();
	});

	const ask = (method: string, params: unknown, server?: string) => {
		if (!language)
			return Promise.reject(new LspError("no_language", t("tools:lspErrors.noLanguage")));
		return channel.request(path, method, params, server);
	};
	const at = (offset: number) => ({ position: document.positionAt(offset) });
	const places = async (method: string, offset: number, extra: object = {}) => {
		const answer = await ask(method, { ...at(offset), ...extra });
		return toLocations(answer.result, answer.externals);
	};

	/** Definitions by name: this file's tags first, else a search. */
	const approximate = async (
		offset: number,
		kind: "definition" | "references",
	): Promise<Location[]> => {
		if (!language || path.startsWith("external:")) return [];
		const syntax = await loadSyntax();
		const text = document.text();
		const name =
			(await syntax.nameAt({ text, language }, offset))?.name ?? wordAt(text, offset).word;
		if (!name) return [];
		if (kind === "definition") {
			const local = await syntax.definitions(name, { text, language });
			if (local.length)
				return local.map((tag) => ({
					path,
					line: document.positionAt(tag.nameFrom).line + 1,
					character: document.positionAt(tag.nameFrom).character,
					approximate: true,
				}));
		}
		const found = await findWord(projectId, name).catch(() => []);
		return found.slice(0, 200).map((spot) => Object.assign(spot, { approximate: true }));
	};

	return {
		diagnostics(listen) {
			diagnosticListeners.add(listen);
			return () => diagnosticListeners.delete(listen);
		},
		status(listen) {
			statusListeners.add(listen);
			return () => statusListeners.delete(listen);
		},
		async hover(offset) {
			const answer = await ask("textDocument/hover", at(offset)).catch(() => null);
			const result = answer?.result as
				| { contents?: Parameters<typeof hoverMarkdown>[0]; range?: { start: never; end: never } }
				| null
				| undefined;
			const markdown = hoverMarkdown(result?.contents);
			if (!markdown.trim()) return null;
			const range = result?.range as
				| { start: { line: number; character: number }; end: { line: number; character: number } }
				| undefined;
			return {
				from: range ? document.offsetAt(range.start) : offset,
				to: range ? document.offsetAt(range.end) : offset,
				markdown,
			};
		},
		definition: (offset) =>
			orElse(
				() => places("textDocument/definition", offset),
				() => approximate(offset, "definition"),
			),
		references: (offset) =>
			orElse(
				() => places("textDocument/references", offset, { context: { includeDeclaration: true } }),
				() => approximate(offset, "references"),
			),
		typeDefinition: (offset) => places("textDocument/typeDefinition", offset).catch(() => []),
		implementation: (offset) => places("textDocument/implementation", offset).catch(() => []),
		async highlights(offset) {
			const answer = await ask("textDocument/documentHighlight", at(offset)).catch(() => null);
			if (!Array.isArray(answer?.result)) return [];
			return (answer.result as { range: { start: never; end: never } }[]).map(
				(item): TextRange => ({
					from: document.offsetAt(item.range.start),
					to: document.offsetAt(item.range.end),
				}),
			);
		},
		async completion(offset, explicit) {
			const answer = await ask("textDocument/completion", {
				...at(offset),
				context: { triggerKind: explicit ? 1 : 1 },
			}).catch(() => null);
			if (!answer) return null;
			return toCompletions(answer.result, document, wordAt(document.text(), offset).from);
		},
		async signatureHelp(offset) {
			const answer = await ask("textDocument/signatureHelp", at(offset)).catch(() => null);
			return answer ? toSignature(answer.result) : null;
		},
		async format() {
			const indent = file.indent?.() ?? { tabSize: 4, insertSpaces: true };
			const answer = await ask("ahab/format", { options: indent });
			const result = answer.result as { edits?: Parameters<typeof toTextEdits>[0]; text?: string };
			if (result.text !== undefined) return replaceEdits(document.text(), result.text);
			return toTextEdits(result.edits ?? [], document);
		},
		async symbols() {
			const answer = await ask("textDocument/documentSymbol", {});
			return toSymbols(answer.result, document);
		},
		async prepareRename(offset) {
			const { word, from } = wordAt(document.text(), offset);
			const answer = await ask("textDocument/prepareRename", at(offset)).catch(() => null);
			const result = answer?.result as
				| { range?: { start: never; end: never }; placeholder?: string; start?: never; end?: never }
				| null
				| undefined;
			if (result?.range)
				return {
					from: document.offsetAt(result.range.start),
					to: document.offsetAt(result.range.end),
					name: result.placeholder ?? word,
				};
			if (result?.start && result.end)
				return {
					from: document.offsetAt(result.start),
					to: document.offsetAt(result.end),
					name: word,
				};
			return word ? { from, to: from + word.length, name: word } : null;
		},
		async rename(offset, name) {
			const answer = await ask("textDocument/rename", { ...at(offset), newName: name });
			return toFileEdits(answer.result);
		},
		async codeActions(from, to) {
			const range = { start: document.positionAt(from), end: document.positionAt(to) };
			const lines = [range.start.line, range.end.line];
			// Plugins' fixes (plugin API 0.1.3), kept with their problems: live
			// ones first, else what Problems stored.
			const fromPlugins = pluginFixes(
				[...bySource, ...[...stored].filter(([source]) => !bySource.has(source))],
				lines[0] ?? 0,
				lines[1] ?? 0,
			).map((fix): CodeAction => ({
				title: fix.title,
				kind: "quickfix",
				source: fix.source,
				resolve: () => Promise.resolve([{ path, edits: fix.edits }]),
			}));
			if (!language) return fromPlugins;
			const diagnostics = [...bySource.values()]
				.flat()
				.filter(
					(item) =>
						item.range.end.line >= (lines[0] ?? 0) && item.range.start.line <= (lines[1] ?? 0),
				);
			const answer = await ask("textDocument/codeAction", {
				range,
				context: { diagnostics },
			}).catch(() => null);
			const list = (Array.isArray(answer?.result) ? answer.result : []) as {
				title: string;
				kind?: string;
				isPreferred?: boolean;
				edit?: unknown;
				command?: { command: string; arguments?: unknown[] } | string;
				arguments?: unknown[];
				ahabServer?: string;
				ahabSource?: string;
				data?: unknown;
			}[];
			return [
				...fromPlugins,
				...list.map((action): CodeAction => ({
					title: action.title,
					kind: action.kind,
					preferred: action.isPreferred,
					source: action.ahabSource,
					async resolve() {
						let resolved = action;
						if (!resolved.edit && !resolved.command && action.data !== undefined) {
							const { ahabServer: _server, ahabSource: _source, ...plain } = action;
							const resolution = await ask("codeAction/resolve", plain, action.ahabServer).catch(
								() => null,
							);
							if (resolution?.result) resolved = { ...action, ...(resolution.result as object) };
						}
						const edits = resolved.edit ? toFileEdits(resolved.edit) : [];
						const command =
							typeof resolved.command === "string"
								? { command: resolved.command, arguments: resolved.arguments }
								: resolved.command;
						if (command) {
							// The server makes its edits through `workspace/applyEdit`.
							channel.commander = open;
							await ask(
								"workspace/executeCommand",
								{ command: command.command, arguments: command.arguments ?? [] },
								action.ahabServer,
							).catch(() => null);
						}
						return edits;
					},
				})),
			];
		},
		applyEdits: applyFiles,
		formatOnSave: () => Boolean(frame?.formatOnSave),
		saved: () => channel.save(path),
		update(change: DocumentChange) {
			if (!language) {
				sent = document.text();
				return;
			}
			// Each edit in the text before it: sent last first, so earlier
			// positions stay right.
			const positions = positionsOf(sent);
			const changes = change.edits
				.toSorted((a, b) => b.from - a.from)
				.map((edit) => ({
					range: { start: positions.positionAt(edit.from), end: positions.positionAt(edit.to) },
					text: edit.insert,
				}));
			for (const edit of change.edits.toSorted((a, b) => b.from - a.from))
				sent = sent.slice(0, edit.from) + edit.insert + sent.slice(edit.to);
			version += 1;
			channel.change(path, version, changes);
		},
		dispose() {
			disposed = true;
			stopStored?.();
			statusListeners.clear();
			diagnosticListeners.clear();
			if (language) channel.close(path, open);
		},
	};
}
