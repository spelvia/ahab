/**
 * A daemon entry's side of the protocol: JSON-RPC 2.0 over the framed pipe
 * (see `framing.ts`), the plugin's handlers, its channels.
 */
import {
	type ChannelDefinition,
	type CommandDefinition,
	type DeliveredNotification,
	type EventDefinition,
	type HookAnswer,
	type Hooks,
	jsonSchema,
	type NotificationLevel,
	type NotificationTarget,
	type z,
} from "../shared/index.ts";
import { Decoder, encode, type Frame } from "./framing.ts";

/**
 * The plugin API's version this SDK speaks. 0.1.3 added the diagnostics API;
 * 0.1.4 notifications.
 */
export const PROTOCOL_VERSION = "0.1.4";

/** Where a command was asked to run. */
export interface CallContext {
	/** The project it was run for, with its folder on this machine. */
	project?: { id: string; folder: string };
	/** Aborted when the caller goes away. */
	signal: AbortSignal;
	/** Reports how far along it is: a fraction from 0 to 1, or a sentence. */
	progress(value: number | string): void;
	/** The plugin's settings in effect where it runs (for the project, if one). */
	settings: Record<string, unknown>;
}

/** One stream with a web entry. */
export interface Channel {
	readonly id: string;
	/** Sends bytes (or text, as UTF-8). */
	write(data: Uint8Array | string): void;
	onData(listener: (data: Uint8Array) => void): void;
	onClose(listener: () => void): void;
	/** Ends it, with a reason the web entry is shown. */
	close(reason?: string): void;
}

type Remove = () => void;
type Settings = Record<string, unknown>;

/** A place in a file: lines and characters count from 0, as in LSP. */
export interface Position {
	line: number;
	character?: number;
}

/** A problem's severity. */
export type Severity = "error" | "warning" | "information" | "hint";

/** One problem in a file, in the form every source of problems in Ahab shares (LSP's). */
export interface Diagnostic {
	/** Where it is; `end` defaults to `start`. */
	range: { start: Position; end?: Position };
	/** `error` when left out. */
	severity?: Severity;
	message: string;
	/** The rule's name or number (`deprecation`, `E0308`). */
	code?: string | number;
	/** Fixes for it, offered in the editor as quick fixes (⌘.) in Edit mode. */
	fixes?: Fix[];
}

/** A quick fix: its title, and the edits it makes to the problem's file. */
export interface Fix {
	title: string;
	/** Each replaces `range` (`end` defaults to `start`) with `newText`. */
	edits: { range: { start: Position; end?: Position }; newText: string }[];
}

/** A project, by its ID or as a check hands it over. */
export type ProjectRef = string | { id: string };

/**
 * A plugin's set of problems, as VS Code's `DiagnosticCollection`: its
 * problems show in Problems, the editor and a turn's difference, with the
 * source `<plugin>.<name>`. Paths are relative to the project (absolute
 * ones inside it are accepted); others are dropped. Cleared when the
 * plugin stops or is disabled.
 */
export interface DiagnosticCollection {
	readonly name: string;
	/** A file's problems, replacing what this collection said of it. */
	set(project: ProjectRef, path: string, diagnostics: Diagnostic[]): void;
	/** Several files' problems at once. */
	set(project: ProjectRef, files: Record<string, Diagnostic[]>): void;
	/** Forgets a file's problems. */
	delete(project: ProjectRef, path: string): void;
	/** Forgets every problem in a project, or everywhere. */
	clear(project?: ProjectRef): void;
}

/** What a check on demand is for. */
export interface CheckRequest {
	/** The project, with its folder on this machine. */
	project: { id: string; folder: string };
	/** The repositories and worktrees to check: folders relative to the project (`""` is its own folder). */
	roots: string[];
	/** Why: Check project (`request`) or an agent's turn (`agentTurn`). */
	reason: "request" | "agentTurn" | (string & {});
}

/** What a daemon entry's function gets. */
export interface NodeContext {
	/** The plugin's ID. */
	readonly plugin: string;
	/** The machine's name. */
	readonly machine: string;
	readonly ahabVersion: string;
	/** The settings in effect on this machine. */
	settings(): Settings;
	onSettings(listener: (settings: Settings) => void): Remove;
	commands: {
		handle<I extends z.ZodType, O extends z.ZodType>(
			definition: CommandDefinition<I, O>,
			handler: (input: z.output<I>, call: CallContext) => z.input<O> | Promise<z.input<O>>,
		): Remove;
		/** Runs another plugin's command, or Ahab's (`terminals.create`), by its full name. */
		call(
			name: string,
			input: unknown,
			target?: { machine?: string; project?: string },
		): Promise<unknown>;
	};
	channels: {
		handle<P extends z.ZodType>(
			definition: ChannelDefinition<P>,
			handler: (channel: Channel, params: z.output<P>) => void | Promise<void>,
		): Remove;
	};
	hooks: {
		before<H extends keyof Hooks>(
			hook: H,
			handler: (data: Hooks[H]) => HookAnswer<Hooks[H]> | Promise<HookAnswer<Hooks[H]>>,
		): Remove;
	};
	/** Terminals in Ahab that this plugin starts: it sees and stops only its own. */
	terminals: {
		create(options: {
			/** The project (its ID) whose folder it opens in. */
			project: string;
			title?: string;
			/** A command for the login shell; with `args`, the words after it, quoted. */
			command?: string;
			args?: string[];
			cols?: number;
			rows?: number;
		}): Promise<{ session: string; terminal: TerminalInfo }>;
		list(): Promise<TerminalInfo[]>;
		kill(terminal: string): Promise<void>;
	};
	events: {
		publish<D extends z.ZodType>(definition: EventDefinition<D>, data: z.input<D>): void;
		/** Ahab's events (`terminal.exited`) and other plugins' (`docker.container.changed`). */
		on(name: string, listener: (data: unknown) => void): Remove;
	};
	log: {
		info(...parts: unknown[]): void;
		warn(...parts: unknown[]): void;
		error(...parts: unknown[]): void;
	};
	/** Problems from this plugin (plugin API 0.1.3). */
	diagnostics: {
		/** The collection called `name` (lower-case letters, digits and dashes). */
		collection(name: string): DiagnosticCollection;
		/** Runs for Check project and after agents' turns; Ahab waits for it. */
		onCheck(handler: (check: CheckRequest) => void | Promise<void>): Remove;
	};
	/** Notifications (plugin API 0.1.4). */
	notifications: {
		/**
		 * Sends one, of a kind the manifest declares (`notifications`); its
		 * full kind is `<plugin>.<kind>`. Answers its ID, or `null` when it
		 * was not kept (turned off, or a hook dropped it).
		 */
		send(notification: SendNotification): Promise<{ id: string | null }>;
		/**
		 * Delivers every notification somewhere (ntfy, Slack), while the
		 * person shows notifications outside the app; `id` is one the
		 * manifest declares (`destinations`). Throw to say it failed.
		 */
		addDestination(destination: Destination): Remove;
	};
	/** Runs when the plugin stops. */
	onStop(listener: () => void | Promise<void>): void;
}

/** What `notifications.send` takes. */
export interface SendNotification {
	/** A declared kind, without the plugin's ID: `pipeline.failed`. */
	kind: string;
	title: string;
	body?: string;
	level?: NotificationLevel;
	/** The project it is about, by ID. */
	project?: string;
	target?: NotificationTarget;
	/** A newer unread one of the same group replaces the older. */
	groupKey?: string;
}

/** A place notifications go. */
export interface Destination {
	id: string;
	deliver(
		notification: DeliveredNotification,
		context: { settings: Settings },
	): void | Promise<void>;
}

/** A terminal as Ahab lists it. */
export interface TerminalInfo {
	id: string;
	sessionId: string;
	projectId: string;
	command: string | null;
	state: "running" | "exited";
	exitCode: number | null;
	title: string | null;
}

/** A daemon entry: registers what it handles; may answer a cleanup. */
export type Setup = (ahab: NodeContext) => void | Remove | Promise<void | Remove>;

/** The pipe the runtime speaks over. */
export interface Io {
	write(bytes: Uint8Array): void;
	/** Calls `listener` with each chunk read; `end` once the pipe closes. */
	read(listener: (chunk: Uint8Array) => void, end: () => void): void;
	exit(code: number): void;
}

interface RpcError {
	code: number;
	message: string;
	data?: { code: string };
}

interface Message {
	jsonrpc: "2.0";
	id?: number | string;
	method?: string;
	params?: Record<string, unknown>;
	result?: unknown;
	error?: RpcError;
}

/** An error whose `code` Ahab shows (`command_failed`, `invalid_input`…). */
export class PluginError extends Error {
	constructor(
		readonly code: string,
		message: string,
	) {
		super(message);
	}
}

function text(parts: unknown[]): string {
	return parts
		.map((part) =>
			typeof part === "string"
				? part
				: part instanceof Error
					? (part.stack ?? part.message)
					: JSON.stringify(part),
		)
		.join(" ");
}

/** A project's ID, however it was given. */
function projectId(project: ProjectRef): string {
	return typeof project === "string" ? project : project.id;
}

/** Sets `key` in `map`; answers what removes it again (if still that value). */
function remover<K, V>(map: Map<K, V>, key: K, value: V): Remove {
	map.set(key, value);
	return () => {
		if (map.get(key) === value) map.delete(key);
	};
}

/** Starts a runtime for `setup` over `io`. */
export function createRuntime(setup: Setup, io: Io) {
	const commands = new Map<
		string,
		{
			definition: CommandDefinition<z.ZodType, z.ZodType>;
			handler: (input: unknown, call: CallContext) => unknown;
		}
	>();
	const channelHandlers = new Map<
		string,
		{
			definition: ChannelDefinition<z.ZodType>;
			handler: (channel: Channel, params: unknown) => unknown;
		}
	>();
	const hooks = new Map<string, (data: never) => unknown>();
	const destinations = new Map<string, Destination>();
	const published = new Map<string, EventDefinition<z.ZodType>>();
	const listeners = new Map<string, Set<(data: unknown) => void>>();
	const settingsListeners = new Set<(settings: Settings) => void>();
	const checkers = new Set<(check: CheckRequest) => void | Promise<void>>();
	const stops: (() => void | Promise<void>)[] = [];
	const running = new Map<number | string, AbortController>();
	const open = new Map<
		string,
		{ data: Set<(data: Uint8Array) => void>; close: Set<() => void>; closed: boolean }
	>();
	const pending = new Map<
		number,
		{ resolve: (value: unknown) => void; reject: (error: Error) => void }
	>();
	let nextId = 1;
	let settings: Settings = {};
	let info = { plugin: "", machine: "", ahabVersion: "" };
	let initialized = false;
	// Everything but `initialize` waits until the plugin's function has run.
	let ready!: () => void;
	const whenReady = new Promise<void>((resolve) => {
		ready = resolve;
	});

	const send = (message: Message) =>
		io.write(encode({ body: new TextEncoder().encode(JSON.stringify(message)) }));
	const notify = (method: string, params: Record<string, unknown>) =>
		send({ jsonrpc: "2.0", method, params });
	const request = (method: string, params: Record<string, unknown>) =>
		new Promise<unknown>((resolve, reject) => {
			const id = nextId++;
			pending.set(id, { resolve, reject });
			send({ jsonrpc: "2.0", id, method, params });
		});
	const log = (level: string, parts: unknown[]) => notify("log", { level, message: text(parts) });

	const channel = (id: string): Channel => {
		const state = {
			data: new Set<(data: Uint8Array) => void>(),
			close: new Set<() => void>(),
			closed: false,
		};
		open.set(id, state);
		return {
			id,
			write(data) {
				if (state.closed) return;
				const body = typeof data === "string" ? new TextEncoder().encode(data) : data;
				io.write(encode({ channel: id, body }));
			},
			onData: (listener) => void state.data.add(listener),
			onClose: (listener) => void state.close.add(listener),
			close(reason) {
				if (state.closed) return;
				closeChannel(id);
				notify("channels.close", reason ? { channel: id, reason } : { channel: id });
			},
		};
	};
	const closeChannel = (id: string) => {
		const state = open.get(id);
		if (!state || state.closed) return;
		state.closed = true;
		open.delete(id);
		for (const listener of state.close) listener();
	};

	const context: NodeContext = {
		get plugin() {
			return info.plugin;
		},
		get machine() {
			return info.machine;
		},
		get ahabVersion() {
			return info.ahabVersion;
		},
		settings: () => settings,
		onSettings(listener) {
			settingsListeners.add(listener);
			return () => settingsListeners.delete(listener);
		},
		commands: {
			handle: (definition, handler) =>
				remover(commands, definition.name, {
					definition,
					handler: handler as (input: unknown, call: CallContext) => unknown,
				}),
			call: (name, input, target) => request("commands.call", { name, input, ...target }),
		},
		channels: {
			handle: (definition, handler) =>
				remover(channelHandlers, definition.name, {
					definition,
					handler: handler as (channel: Channel, params: unknown) => unknown,
				}),
		},
		hooks: {
			before(hook, handler) {
				const remove = remover(hooks, hook, handler as (data: never) => unknown);
				// Ahab learns the hooks from `initialize`'s answer; later ones
				// are told.
				const declare = () => {
					if (initialized) notify("hooks.declare", { hooks: [...hooks.keys()] });
				};
				declare();
				return () => {
					remove();
					declare();
				};
			},
		},
		notifications: {
			send: (notification) =>
				request("notifications.send", { ...notification }) as Promise<{ id: string | null }>,
			addDestination: (destination) => remover(destinations, destination.id, destination),
		},
		terminals: {
			create: (options) =>
				request("commands.call", {
					name: "terminals.create",
					input: options,
					project: options.project,
				}) as Promise<{ session: string; terminal: TerminalInfo }>,
			list: () =>
				request("commands.call", { name: "terminals.list", input: {} }) as Promise<TerminalInfo[]>,
			kill: async (terminal) => {
				await request("commands.call", { name: "terminals.kill", input: { terminal } });
			},
		},
		events: {
			publish(definition, data) {
				published.set(definition.name, definition);
				notify("events.publish", {
					name: definition.name,
					data: definition.data.parse(data),
					lasting: definition.lasting ?? false,
				});
			},
			on(name, listener) {
				const set = listeners.get(name) ?? new Set();
				set.add(listener);
				listeners.set(name, set);
				if (initialized) notify("events.subscribe", { names: [...listeners.keys()] });
				return () => set.delete(listener);
			},
		},
		log: {
			info: (...parts) => log("info", parts),
			warn: (...parts) => log("warn", parts),
			error: (...parts) => log("error", parts),
		},
		diagnostics: {
			collection(name) {
				if (!/^[a-z0-9-]{1,64}$/.test(name))
					throw new PluginError(
						"invalid_input",
						`${name} is not a collection name: use lower-case letters, digits and dashes.`,
					);
				return {
					name,
					set(
						project: ProjectRef,
						pathOrFiles: string | Record<string, Diagnostic[]>,
						diagnostics?: Diagnostic[],
					) {
						const files =
							typeof pathOrFiles === "string" ? { [pathOrFiles]: diagnostics ?? [] } : pathOrFiles;
						notify("diagnostics.set", {
							collection: name,
							project: projectId(project),
							files,
						});
					},
					delete(project, path) {
						notify("diagnostics.delete", {
							collection: name,
							project: projectId(project),
							paths: [path],
						});
					},
					clear(project) {
						notify(
							"diagnostics.clear",
							project === undefined
								? { collection: name }
								: { collection: name, project: projectId(project) },
						);
					},
				} as DiagnosticCollection;
			},
			onCheck(handler) {
				checkers.add(handler);
				if (initialized) notify("diagnostics.declare", { check: true });
				return () => {
					checkers.delete(handler);
					if (initialized) notify("diagnostics.declare", { check: checkers.size > 0 });
				};
			},
		},
		onStop: (listener) => void stops.push(listener),
	};

	/** What the host is told the plugin offers. */
	const declared = () => ({
		protocol: PROTOCOL_VERSION,
		commands: [...commands.values()].map(({ definition }) => ({
			name: definition.name,
			description: definition.description ?? null,
			input: jsonSchema(definition.input, "input"),
			output: jsonSchema(definition.output),
		})),
		channels: [...channelHandlers.values()].map(({ definition }) => ({
			name: definition.name,
			params: jsonSchema(definition.params, "input"),
		})),
		hooks: [...hooks.keys()],
		events: [...listeners.keys()],
		diagnostics: { check: checkers.size > 0 },
	});

	const methods: Record<
		string,
		(params: Record<string, unknown>, id?: number | string) => unknown
	> = {
		async initialize(params) {
			if (initialized)
				throw new PluginError("already_initialized", "The plugin is already running.");
			info = {
				plugin: String(params.plugin ?? ""),
				machine: String(params.machine ?? ""),
				ahabVersion: String(params.ahabVersion ?? ""),
			};
			settings = (params.settings as Settings | undefined) ?? {};
			try {
				const cleanup = await setup(context);
				if (typeof cleanup === "function") stops.push(cleanup);
			} finally {
				initialized = true;
				ready();
			}
			return declared();
		},
		async "commands.run"(params, id) {
			const name = String(params.name);
			const command = commands.get(name);
			if (!command) throw new PluginError("command_unknown", `This plugin has no command ${name}.`);
			const parsed = command.definition.input.safeParse(params.input ?? {});
			if (!parsed.success) throw new PluginError("invalid_input", parsed.error.message);
			const abort = new AbortController();
			if (id !== undefined) running.set(id, abort);
			const raw = (params.context as Record<string, unknown> | undefined) ?? {};
			const call: CallContext = {
				signal: abort.signal,
				settings: (raw.settings as Settings | undefined) ?? settings,
				progress: (value) => notify("progress", { id: id ?? null, value }),
				...(raw.project ? { project: raw.project as { id: string; folder: string } } : {}),
			};
			try {
				return await command.handler(parsed.data, call);
			} finally {
				if (id !== undefined) running.delete(id);
			}
		},
		"$/cancelRequest"(params) {
			running.get(params.id as number | string)?.abort();
		},
		async "channels.open"(params) {
			const name = String(params.name);
			const handler = channelHandlers.get(name);
			if (!handler) throw new PluginError("channel_unknown", `This plugin has no channel ${name}.`);
			const parsed = handler.definition.params.safeParse(params.params ?? {});
			if (!parsed.success) throw new PluginError("invalid_input", parsed.error.message);
			const opened = channel(String(params.channel));
			// Answered at once; what the handler sends follows.
			void Promise.resolve(handler.handler(opened, parsed.data)).catch((error: unknown) => {
				log("error", [`channel ${name}:`, error]);
				opened.close(error instanceof Error ? error.message : String(error));
			});
			return {};
		},
		"channels.close"(params) {
			closeChannel(String(params.channel));
		},
		async "hooks.before"(params) {
			const handler = hooks.get(String(params.hook));
			if (!handler) return { action: "allow" };
			return await handler(params.data as never);
		},
		async "notifications.deliver"(params) {
			const id = String(params.destination);
			const destination = destinations.get(id);
			if (!destination)
				throw new PluginError("destination_unknown", `This plugin has no destination ${id}.`);
			await destination.deliver(params.notification as DeliveredNotification, {
				settings: (params.settings as Settings | undefined) ?? settings,
			});
			return {};
		},
		"events.deliver"(params) {
			for (const listener of listeners.get(String(params.name)) ?? []) {
				try {
					listener(params.data);
				} catch (error) {
					log("error", [`event ${String(params.name)}:`, error]);
				}
			}
		},
		async "diagnostics.check"(params) {
			const check = params as unknown as CheckRequest;
			const failures: unknown[] = [];
			await Promise.all(
				[...checkers].map(async (handler) => {
					try {
						await handler(check);
					} catch (error) {
						failures.push(error);
						log("error", ["check:", error]);
					}
				}),
			);
			if (failures.length)
				throw new PluginError("check_failed", `The plugin's check failed: ${text(failures)}`);
			return {};
		},
		"settings.changed"(params) {
			settings = (params.settings as Settings | undefined) ?? {};
			for (const listener of settingsListeners) listener(settings);
		},
		async shutdown() {
			for (const id of open.keys()) closeChannel(id);
			// Last registered, first stopped; one after another.
			for (const stop of stops.toReversed()) {
				try {
					// oxlint-disable-next-line no-await-in-loop
					await stop();
				} catch (error) {
					log("error", ["stopping:", error]);
				}
			}
			setTimeout(() => io.exit(0), 0);
			return {};
		},
	};

	const answer = async (message: Message) => {
		const method = message.method ?? "";
		const handler = methods[method];
		const isRequest = message.id !== undefined;
		if (!handler) {
			if (isRequest) {
				send({
					jsonrpc: "2.0",
					id: message.id!,
					error: {
						code: -32601,
						message: `Unknown method ${method}.`,
						data: { code: "method_unknown" },
					},
				});
			}
			return;
		}
		try {
			if (method !== "initialize") await whenReady;
			const result = await handler(message.params ?? {}, message.id);
			if (isRequest) send({ jsonrpc: "2.0", id: message.id!, result: result ?? null });
		} catch (error) {
			if (!isRequest) {
				log("error", [`${method}:`, error]);
				return;
			}
			const code = error instanceof PluginError ? error.code : "command_failed";
			const said = error instanceof Error ? error.message : String(error);
			send({
				jsonrpc: "2.0",
				id: message.id!,
				error: { code: -32000, message: said, data: { code } },
			});
		}
	};

	const receive = (frame: Frame) => {
		if (frame.channel) {
			// Channels open only after `initialize`, so no wait is needed.
			for (const listener of open.get(frame.channel)?.data ?? []) listener(frame.body);
			return;
		}
		let message: Message;
		try {
			message = JSON.parse(new TextDecoder().decode(frame.body)) as Message;
		} catch {
			log("error", ["a message from Ahab was not JSON"]);
			return;
		}
		if (message.method === undefined && typeof message.id === "number") {
			const waiting = pending.get(message.id);
			if (!waiting) return;
			pending.delete(message.id);
			if (message.error) {
				const failure = new PluginError(
					message.error.data?.code ?? "failed",
					message.error.message,
				);
				waiting.reject(failure);
			} else waiting.resolve(message.result);
			return;
		}
		void answer(message);
	};

	const decoder = new Decoder();
	io.read(
		(chunk) => {
			let frames: Frame[];
			try {
				frames = decoder.push(chunk);
			} catch (error) {
				log("error", ["the pipe from Ahab is broken:", error]);
				io.exit(1);
				return;
			}
			for (const frame of frames) receive(frame);
		},
		() => io.exit(0),
	);

	return { context, log };
}
