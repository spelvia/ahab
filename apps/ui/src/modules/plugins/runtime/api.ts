/**
 * What a plugin's web entry gets from Ahab: its context (commands,
 * channels, events, terminals, and what it adds to the web app), and the
 * plugins to load.
 */
import type { Ref, Target, WebChannel, WebContext } from "@ahab/plugin/web";
import type { ComponentType, ReactNode } from "react";

import {
	add,
	contributions as registry,
	type PluginView,
} from "@/modules/plugins/runtime/components/registry";
import { scoped, scopedIcon } from "@/modules/plugins/runtime/components/scope";
import { currentHost, windowTabId } from "@/modules/plugins/runtime/model";
import { startTerminalIn } from "@/modules/sessions";
import { onEvent } from "@/shared/api/events";
import { ApiError } from "@/shared/api/fetch";
import type { Problem } from "@/shared/api/generated/ahab.schemas";
import { runPluginCommand } from "@/shared/api/generated/plugins-runtime/plugins-runtime";
import { t } from "@/shared/i18n";

/** A command or channel's plugin and name: a full name, or one of `plugin`'s own. */
export function resolve(plugin: string, ref: Ref): [string, string] {
	if (typeof ref !== "string") return [plugin, ref.name];
	const dot = ref.indexOf(".");
	if (dot < 0) return [plugin, ref];
	return [ref.slice(0, dot), ref.slice(dot + 1)];
}

/** Opens a plugin's channel: a WebSocket to its daemon entry. */
function openChannel(plugin: string, name: string, params: unknown, target: Target): WebChannel {
	const query = new URLSearchParams();
	if (target.machine) query.set("machine", target.machine);
	if (target.project) query.set("project", target.project);
	if (params !== undefined) query.set("params", JSON.stringify(params));
	const scheme = globalThis.location?.protocol === "https:" ? "wss:" : "ws:";
	const url = `${scheme}//${globalThis.location?.host}/api/v1/plugins/${plugin}/channels/${name}?${query}`;
	const socket = new WebSocket(url);
	socket.binaryType = "arraybuffer";
	const listeners = new Set<(bytes: Uint8Array) => void>();
	const closed = new Set<(reason?: string) => void>();
	const queued: (Uint8Array<ArrayBuffer> | string)[] = [];
	let ended = false;
	let reason: string | undefined;
	const end = (why?: string) => {
		if (ended) return;
		ended = true;
		for (const listener of closed) listener(why);
	};
	socket.addEventListener("open", () => {
		for (const item of queued.splice(0)) socket.send(item);
	});
	socket.addEventListener("message", (message) => {
		if (typeof message.data === "string") {
			const frame = JSON.parse(message.data) as { type?: string; reason?: string };
			if (frame.type === "end") reason = frame.reason || undefined;
			return;
		}
		const bytes = new Uint8Array(message.data as ArrayBuffer);
		for (const listener of listeners) listener(bytes);
	});
	socket.addEventListener("close", (event) =>
		end(reason ?? (event.wasClean ? undefined : t("plugins:runtime.connectionLost"))),
	);
	return {
		write(data) {
			const item = typeof data === "string" ? data : new Uint8Array(data);
			if (socket.readyState === WebSocket.OPEN) socket.send(item);
			else if (socket.readyState === WebSocket.CONNECTING) queued.push(item);
		},
		onData(listener) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		onClose(listener) {
			closed.add(listener);
			return () => closed.delete(listener);
		},
		close() {
			socket.close();
			end();
		},
	};
}

const project = () => currentHost().project;

/**
 * The context a plugin's web entry is called with. Everything it renders
 * (components and icons) is wrapped in its scope, where its stylesheet
 * applies.
 */
export function createContext(plugin: string): WebContext {
	const key = (id: string) => `plugin:${plugin}:${id}`;
	const icon = (value: ReactNode) => scopedIcon(plugin, value);
	return {
		plugin,
		project,
		commands: {
			async call<Output>(command: Ref, input?: unknown, target: Target = {}) {
				const [owner, name] = resolve(plugin, command);
				const answer = await runPluginCommand(owner, name, {
					input: input ?? {},
					machine: target.machine,
					project: target.project,
				});
				return answer.output as Output;
			},
		},
		channels: {
			open(channel, params, target = {}) {
				const [owner, name] = resolve(plugin, channel);
				return openChannel(owner, name, params, target);
			},
		},
		events: { on: onEvent },
		views: {
			add: (view) =>
				add("views", {
					...view,
					key: key(view.id),
					plugin,
					icon: icon(view.icon),
					Component: view.Component && scoped(plugin, view.Component),
				} satisfies PluginView),
		},
		activity: {
			add: (item) =>
				add("views", {
					key: key(item.id),
					plugin,
					id: item.id,
					title: item.title,
					icon: icon(item.icon),
					opens: item.opens,
				}),
		},
		windows: {
			add: (window) =>
				add("windows", {
					plugin,
					kind: window.kind,
					title: window.title as (params: unknown) => string,
					single: window.single,
					Component: scoped(plugin, window.Component as ComponentType<{ params: unknown }>),
				}),
			open(kind, params) {
				openWindow(plugin, kind, params);
			},
		},
		newWindow: {
			add: (item) =>
				add("newWindow", { ...item, key: key(item.id), plugin, icon: icon(item.icon) }),
		},
		palette: {
			add: (item) => add("palette", { ...item, key: key(item.id), plugin }),
		},
		status: {
			add: (item) =>
				add("status", {
					...item,
					key: key(item.id),
					plugin,
					Component: scoped(plugin, item.Component),
				}),
		},
		settings: {
			page: (page) => add("settings", { plugin, Component: scoped(plugin, page.Component) }),
		},
		terminals: {
			async start(options) {
				const where = options.project ?? project()?.id;
				if (!where) throw new Error(t("plugins:runtime.noProject"));
				const session = await startTerminalIn(where, {
					title: options.title,
					command: options.command,
					context: options.context,
					cols: 120,
					rows: 32,
				}).catch((error: unknown) => {
					throw error instanceof ApiError
						? new Error((error.body as Problem | undefined)?.detail ?? error.message)
						: error;
				});
				if (options.show !== false)
					currentHost().openSession({ id: session.id, title: session.title });
				return { session: session.id, terminal: session.terminal?.id ?? "" };
			},
		},
	};
}

/** Opens a plugin's window in the tiles (or brings a single one forward). */
export function openWindow(plugin: string, kind: string, params: unknown) {
	const { windows } = registry();
	const found = windows.find((window) => window.plugin === plugin && window.kind === kind);
	if (!found) return;
	currentHost().openWindow({
		id: windowTabId(plugin, kind, found.single),
		component: "plugin",
		title: found.title(params),
		params: { plugin, kind, params },
	});
}
