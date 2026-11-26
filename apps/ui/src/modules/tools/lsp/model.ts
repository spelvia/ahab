/**
 * LSP's shapes in the editor's terms: positions to offsets,
 * diagnostics, hovers, places, symbols, completions, edits, and a file's
 * language status from what its daemon reports.
 */
import type {
	Completions,
	Diagnostic,
	DiagnosticTag,
	DocumentSymbol,
	FileEdits,
	LanguageStatus,
	LineRange,
	Location,
	Position,
	Severity,
	SignatureHelp,
	TextEdit,
} from "@/shared/editor/model";
import { t } from "@/shared/i18n";

/** Offsets and positions of one document. */
export interface Positions {
	positionAt(offset: number): Position;
	offsetAt(position: Position): number;
}

/** An LSP diagnostic, as servers send it. */
export interface LspDiagnostic {
	range: LineRange;
	severity?: number;
	code?: string | number;
	codeDescription?: { href?: string };
	source?: string;
	message: string;
	/** 1: unnecessary, 2: deprecated. */
	tags?: number[];
	/** Kept as the source gave it: a plugin's fixes are `data.fixes`. */
	data?: unknown;
}

/** A plugin's fix for a problem (plugin API 0.1.3): edits to its file. */
export interface PluginFix {
	title: string;
	/** The plugin's collection: `<plugin>.<collection>`. */
	source: string;
	edits: { range: LineRange; newText: string }[];
}

/** A position a plugin gave: `character` defaults to 0. */
function pluginPosition(value: unknown): Position | undefined {
	const at = value as { line?: unknown; character?: unknown } | null;
	if (typeof at?.line !== "number" || at.line < 0) return undefined;
	const character = typeof at.character === "number" && at.character >= 0 ? at.character : 0;
	return { line: Math.floor(at.line), character: Math.floor(character) };
}

/**
 * The fixes plugins attached to the problems on lines `from`–`to` (from
 * 0), each with its edits in LSP's form; a fix with no valid edit, or an
 * edit without text, is left out.
 */
export function pluginFixes(
	bySource: Iterable<[string, LspDiagnostic[]]>,
	from: number,
	to: number,
): PluginFix[] {
	const found: PluginFix[] = [];
	for (const [source, items] of bySource)
		for (const item of items) {
			if (item.range.end.line < from || item.range.start.line > to) continue;
			const fixes = (item.data as { fixes?: unknown } | null | undefined)?.fixes;
			if (!Array.isArray(fixes)) continue;
			for (const fix of fixes as { title?: unknown; edits?: unknown }[]) {
				if (typeof fix?.title !== "string" || !fix.title || !Array.isArray(fix.edits)) continue;
				const edits = (
					fix.edits as { range?: { start?: unknown; end?: unknown }; newText?: unknown }[]
				).flatMap((edit) => {
					const start = pluginPosition(edit?.range?.start);
					if (!start || typeof edit.newText !== "string") return [];
					const end = pluginPosition(edit.range?.end) ?? start;
					return [{ range: { start, end }, newText: edit.newText }];
				});
				if (edits.length) found.push({ title: fix.title, source: item.source ?? source, edits });
			}
		}
	return found;
}

const TAGS: Record<number, DiagnosticTag> = { 1: "unnecessary", 2: "deprecated" };

const SEVERITIES: Severity[] = ["error", "error", "warning", "info", "hint"];

/** LSP diagnostics as the editor shows them; `tool` names who said it. */
export function toDiagnostics(items: LspDiagnostic[], doc: Positions, tool: string): Diagnostic[] {
	return items.map((item) => {
		const from = doc.offsetAt(item.range.start);
		const to = doc.offsetAt(item.range.end);
		const tags = (item.tags ?? []).flatMap((tag) => (TAGS[tag] ? [TAGS[tag]] : []));
		const href = item.codeDescription?.href;
		return {
			from,
			to: Math.max(to, from),
			severity: SEVERITIES[item.severity ?? 1] ?? "error",
			message: item.message,
			source: item.source ?? tool,
			...(item.code === undefined || item.code === null ? {} : { code: String(item.code) }),
			...(typeof href === "string" && /^https?:\/\//.test(href) ? { codeHref: href } : {}),
			...(tags.length ? { tags } : {}),
		};
	});
}

type MarkedString = string | { language: string; value: string };
type HoverContents = MarkedString | MarkedString[] | { kind: string; value: string };

/** One marked string as Markdown. */
function markedString(item: MarkedString): string {
	return typeof item === "string" ? item : `\`\`\`${item.language}\n${item.value}\n\`\`\``;
}

/** A hover's contents as Markdown. */
export function hoverMarkdown(contents: HoverContents | undefined | null): string {
	if (!contents) return "";
	const one = markedString;
	if (Array.isArray(contents)) return contents.map(one).join("\n\n");
	if (typeof contents === "object" && "kind" in contents) {
		return contents.kind === "plaintext" ? contents.value : contents.value;
	}
	return one(contents);
}

/** How an external file is shown, as the daemon describes it. */
export interface ExternalInfo {
	label: string;
	name: string;
	display: string;
}

interface LspLocation {
	uri: string;
	range: LineRange;
}

interface LspLocationLink {
	targetUri: string;
	targetSelectionRange?: LineRange;
	targetRange: LineRange;
}

/** Definitions and references as places: project paths or external files. */
export function toLocations(
	result: unknown,
	externals: Record<string, ExternalInfo> = {},
): Location[] {
	if (!result) return [];
	const list = (Array.isArray(result) ? result : [result]) as (LspLocation | LspLocationLink)[];
	return list.flatMap((item) => {
		const path = "targetUri" in item ? item.targetUri : item.uri;
		const range =
			"targetUri" in item ? (item.targetSelectionRange ?? item.targetRange) : item.range;
		if (!path || !range) return [];
		const token = path.startsWith("external:") ? path.slice("external:".length) : undefined;
		const info = token ? externals[token] : undefined;
		return [
			{
				path,
				line: range.start.line + 1,
				character: range.start.character,
				...(info ? { origin: info.label, name: info.name } : {}),
			},
		];
	});
}

/** LSP's symbol kinds, by number, as the outline names them. */
const SYMBOL_KINDS = [
	"",
	"file",
	"module",
	"namespace",
	"package",
	"class",
	"method",
	"property",
	"field",
	"constructor",
	"enum",
	"interface",
	"function",
	"variable",
	"constant",
	"string",
	"number",
	"boolean",
	"array",
	"object",
	"key",
	"null",
	"enum member",
	"struct",
	"event",
	"operator",
	"type parameter",
];

interface LspDocumentSymbol {
	name: string;
	kind: number;
	range: LineRange;
	selectionRange: LineRange;
	children?: LspDocumentSymbol[];
}

interface LspSymbolInformation {
	name: string;
	kind: number;
	location: LspLocation;
}

/** `documentSymbol`'s answer (nested or flat) as the outline's symbols. */
export function toSymbols(result: unknown, doc: Positions): DocumentSymbol[] {
	if (!Array.isArray(result)) return [];
	const convert = (symbol: LspDocumentSymbol): DocumentSymbol => ({
		name: symbol.name,
		kind: SYMBOL_KINDS[symbol.kind] ?? "symbol",
		from: doc.offsetAt(symbol.range.start),
		to: doc.offsetAt(symbol.range.end),
		nameFrom: doc.offsetAt(symbol.selectionRange.start),
		nameTo: doc.offsetAt(symbol.selectionRange.end),
		children: (symbol.children ?? []).map(convert),
	});
	return (result as (LspDocumentSymbol | LspSymbolInformation)[]).map((symbol) =>
		"location" in symbol
			? {
					name: symbol.name,
					kind: SYMBOL_KINDS[symbol.kind] ?? "symbol",
					from: doc.offsetAt(symbol.location.range.start),
					to: doc.offsetAt(symbol.location.range.end),
					nameFrom: doc.offsetAt(symbol.location.range.start),
					nameTo: doc.offsetAt(symbol.location.range.end),
					children: [],
				}
			: convert(symbol),
	);
}

const COMPLETION_KINDS: Record<number, string> = {
	2: "method",
	3: "function",
	4: "function",
	5: "property",
	6: "variable",
	7: "class",
	8: "interface",
	9: "namespace",
	10: "property",
	13: "enum",
	14: "keyword",
	21: "constant",
	22: "type",
	25: "type",
};

interface LspCompletionItem {
	label: string;
	kind?: number;
	detail?: string;
	documentation?: string | { kind: string; value: string };
	insertText?: string;
	textEdit?: { range?: LineRange; insert?: LineRange; newText: string };
}

/** A completion answer; `wordStart` when the items say no range. */
export function toCompletions(
	result: unknown,
	doc: Positions,
	wordStart: number,
): Completions | null {
	if (!result) return null;
	const items = (
		Array.isArray(result) ? result : ((result as { items?: unknown[] }).items ?? [])
	) as LspCompletionItem[];
	if (!items.length) return null;
	const edit = items.find((item) => item.textEdit)?.textEdit;
	const range = edit?.range ?? edit?.insert;
	const from = range ? doc.offsetAt(range.start) : wordStart;
	return {
		from,
		items: items.slice(0, 500).map((item) => ({
			label: item.label,
			detail: item.detail,
			info: typeof item.documentation === "string" ? item.documentation : item.documentation?.value,
			insert: item.textEdit?.newText ?? item.insertText,
			kind: item.kind ? COMPLETION_KINDS[item.kind] : undefined,
		})),
	};
}

interface LspSignatureHelp {
	signatures?: {
		label: string;
		documentation?: string | { value: string };
		parameters?: { label: string | [number, number] }[];
	}[];
	activeSignature?: number;
	activeParameter?: number;
}

export function toSignature(result: unknown): SignatureHelp | null {
	const help = result as LspSignatureHelp | null;
	const signature = help?.signatures?.[help.activeSignature ?? 0];
	if (!signature) return null;
	const parameter = signature.parameters?.[help?.activeParameter ?? 0]?.label;
	let active: [number, number] | undefined;
	if (Array.isArray(parameter)) active = parameter;
	else if (typeof parameter === "string") {
		const at = signature.label.indexOf(parameter);
		if (at >= 0) active = [at, at + parameter.length];
	}
	return {
		label: signature.label,
		active,
		documentation:
			typeof signature.documentation === "string"
				? signature.documentation
				: signature.documentation?.value,
	};
}

interface LspTextEdit {
	range: LineRange;
	newText: string;
}

/** A document's LSP edits as offset edits, in the coordinates before them. */
export function toTextEdits(edits: LspTextEdit[], doc: Positions): TextEdit[] {
	return edits
		.map((edit) => ({
			from: doc.offsetAt(edit.range.start),
			to: doc.offsetAt(edit.range.end),
			insert: edit.newText,
		}))
		.toSorted((a, b) => a.from - b.from);
}

/** Offsets of a text by lines (for files not open in an editor). */
export function positionsOf(text: string): Positions {
	const starts = [0];
	for (let index = 0; index < text.length; index++)
		if (text[index] === "\n") starts.push(index + 1);
	return {
		positionAt(offset) {
			let line = 0;
			while (line + 1 < starts.length && (starts[line + 1] ?? Infinity) <= offset) line++;
			return { line, character: offset - (starts[line] ?? 0) };
		},
		offsetAt({ line, character }) {
			const start = starts[Math.min(line, starts.length - 1)] ?? 0;
			const end = line + 1 < starts.length ? (starts[line + 1] ?? text.length) - 1 : text.length;
			return Math.min(start + character, end);
		},
	};
}

/** `edits` applied to `text`. */
export function applyEdits(text: string, edits: LspTextEdit[]): string {
	const offsets = toTextEdits(edits, positionsOf(text));
	let out = text;
	for (const edit of offsets.toReversed())
		out = out.slice(0, edit.from) + edit.insert + out.slice(edit.to);
	return out;
}

/** The one edit that takes `before` to `after`: their middle that differs. */
export function replaceEdits(before: string, after: string): TextEdit[] {
	if (before === after) return [];
	let start = 0;
	while (start < before.length && start < after.length && before[start] === after[start]) start++;
	let end = 0;
	while (
		end < before.length - start &&
		end < after.length - start &&
		before[before.length - 1 - end] === after[after.length - 1 - end]
	)
		end++;
	return [{ from: start, to: before.length - end, insert: after.slice(start, after.length - end) }];
}

/** A workspace edit (`changes` by path) as edits per file. */
export function toFileEdits(edit: unknown): FileEdits[] {
	const changes = (edit as { changes?: Record<string, LspTextEdit[]> } | null)?.changes ?? {};
	const documentChanges =
		(
			edit as {
				documentChanges?: { textDocument?: { uri: string }; edits?: LspTextEdit[] }[];
			} | null
		)?.documentChanges ?? [];
	const files: FileEdits[] = Object.entries(changes).map(([path, edits]) => ({ path, edits }));
	for (const change of documentChanges) {
		if (change.textDocument && change.edits)
			files.push({ path: change.textDocument.uri, edits: change.edits });
	}
	return files.filter((file) => file.edits.length > 0);
}

// --- The status item ----------------------------------------------------------

/** One tool in a file's status, as the daemon reports it. */
interface StatusTool {
	id: string;
	name: string;
	kind: string;
	state: string;
	version?: string | null;
	detail?: string;
	reason?: string;
	progress?: string;
	installable?: boolean;
	server?: string;
	root?: string;
	config?: string;
	on?: string[];
}

/** A file's status frame. */
export interface StatusFrame {
	path: string;
	language: string;
	root?: { path: string; marker?: string | null } | null;
	environment?: { kind?: string | null; file?: string | null; note?: string | null } | null;
	repository?: string | null;
	tools: StatusTool[];
	configured?: boolean;
	off?: boolean;
	external?: boolean;
	formatOnSave?: boolean;
	formatter?: string | null;
}

const PROBLEM_STATES = new Set(["missing", "cannotRun", "failed"]);

/** How a tool's row reads in the popover. */
function row(tool: StatusTool): LanguageStatus["rows"][number] {
	const ok = PROBLEM_STATES.has(tool.state)
		? false
		: tool.state === "starting" || tool.state === "held"
			? null
			: true;
	let detail: string;
	switch (tool.state) {
		case "starting":
			detail = tool.progress ?? t("tools:languageStatus.row.starting");
			break;
		case "held":
			detail = t("tools:languageStatus.row.notStarted");
			break;
		case "missing":
		case "cannotRun":
		case "failed":
			detail = tool.reason ?? tool.state;
			break;
		default: {
			const parts = [tool.version, tool.progress ?? tool.detail ?? tool.config].filter(Boolean);
			if (tool.kind === "check")
				parts.push(
					tool.on?.includes("save")
						? t("tools:languageStatus.row.checkOnSave")
						: t("tools:languageStatus.row.check"),
				);
			detail = parts.join("   ");
		}
	}
	return { name: tool.name, ok, detail };
}

/**
 * The status item's label and popover from a file's status: `Rust ✓`,
 * `Rust · starting…`, `Rust · 2 issues`, `Java · basic`, `Markdown`.
 */
export function statusOf(
	frame: StatusFrame,
	languageName: string,
): Omit<LanguageStatus, "actions"> {
	const servers = frame.tools.filter((tool) => tool.kind === "languageServer");
	const issues = frame.tools.filter((tool) => PROBLEM_STATES.has(tool.state)).length;
	const running = servers.some((tool) => tool.state === "running");
	const starting = servers.some((tool) => tool.state === "starting");
	// The project starts servers only in pinned repositories, or never.
	const held = servers.find((tool) => tool.state === "held");
	const notes: string[] = [];
	const environment = frame.environment;
	if (environment?.kind)
		notes.push(
			environment.file
				? t("tools:languageStatus.environmentFile", {
						kind: environment.kind,
						file: environment.file,
					})
				: t("tools:languageStatus.environment", { kind: environment.kind }),
		);
	if (environment?.note) notes.push(environment.note);
	if (frame.repository)
		notes.push(t("tools:languageStatus.choicesFrom", { repository: frame.repository }));
	let label: string;
	let tone: LanguageStatus["tone"];
	if (frame.external) {
		label = t("tools:languageStatus.outside", { language: languageName });
		tone = "plain";
	} else if (frame.off) {
		label = t("tools:languageStatus.off", { language: languageName });
		tone = "plain";
	} else if (!frame.tools.length) {
		label = languageName;
		tone = "plain";
	} else if (held && !running && !starting) {
		label = t("tools:languageStatus.notStarted", { language: languageName });
		tone = "plain";
		if (held.reason) notes.push(held.reason);
	} else if (starting && !running) {
		label = t("tools:languageStatus.starting", { language: languageName });
		tone = "busy";
	} else if (!running && servers.length + frame.tools.length > 0 && !servers.length) {
		label = issues
			? t("tools:languageStatus.issues", { language: languageName, count: issues })
			: t("tools:languageStatus.ok", { language: languageName });
		tone = issues ? "warning" : "ok";
	} else if (!running) {
		label = t("tools:languageStatus.basic", { language: languageName });
		tone = "warning";
		notes.push(t("tools:languageStatus.approximateUntil"));
	} else if (issues) {
		label = t("tools:languageStatus.issues", { language: languageName, count: issues });
		tone = "warning";
	} else {
		label = t("tools:languageStatus.ok", { language: languageName });
		tone = "ok";
	}
	const root = frame.root
		? `${frame.root.path ? `${frame.root.path}/` : "./"}${frame.root.marker ? ` (${frame.root.marker})` : ""}`
		: undefined;
	return { label, tone, title: languageName, root, rows: frame.tools.map(row), notes };
}

/** The status of a language nothing serves: "highlighting only". */
export function plainStatus(languageName: string): Omit<LanguageStatus, "actions"> {
	return { label: languageName, tone: "plain", title: languageName, rows: [], notes: [] };
}

/**
 * A stored problem (the project's Problems, 1-based) as the LSP
 * diagnostic it was: for tools that report a file without its window's
 * channel (checks run on save or by hand).
 */
export function storedDiagnostic(problem: {
	line: number;
	column: number;
	endLine: number;
	endColumn: number;
	severity: string;
	code?: string | null;
	source: string;
	message: string;
	/** A plugin's fixes (plugin API 0.1.3). */
	fixes?: unknown[] | null;
}): LspDiagnostic {
	const severity = ["error", "warning", "information", "hint"].indexOf(problem.severity) + 1;
	return {
		range: {
			start: { line: Math.max(0, problem.line - 1), character: Math.max(0, problem.column - 1) },
			end: {
				line: Math.max(0, problem.endLine - 1),
				character: Math.max(0, problem.endColumn - 1),
			},
		},
		severity: severity || 1,
		...(problem.code ? { code: problem.code } : {}),
		source: problem.source,
		message: problem.message,
		...(problem.fixes?.length ? { data: { fixes: problem.fixes } } : {}),
	};
}
