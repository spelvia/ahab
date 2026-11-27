/**
 * Ahab's and plugins' events, pushed by the server: one stream for the
 * whole web app, shared by every plugin that listens.
 */

import { onResume, retryDelay } from "@/shared/api/resume";

type Listener = (data: unknown) => void;
type ConnectionListener = (open: boolean) => void;

const listeners = new Map<string, Set<Listener>>();
const connectionListeners = new Set<ConnectionListener>();
let source: EventSource | undefined;
let retry: ReturnType<typeof setTimeout> | undefined;
let failures = 0;
let readyFrame: unknown;
let stopResume: (() => void) | undefined;

/** Back on the page: reconnects now, instead of after its retry delay. */
function resume(stale: boolean) {
	if (listeners.size === 0) return;
	if (source && !stale) return;
	clearTimeout(retry);
	retry = undefined;
	if (source) {
		source.close();
		source = undefined;
		readyFrame = undefined;
	}
	failures = 0;
	connect();
}

function connect() {
	if (source || retry !== undefined || typeof EventSource === "undefined") return;
	stopResume ??= onResume(resume);
	const opened = new EventSource("/api/v1/plugin-events?metadata=true");
	source = opened;
	opened.addEventListener("open", () => {
		failures = 0;
		for (const listener of connectionListeners) listener(true);
	});
	opened.addEventListener("message", (message) => {
		if (source !== opened) return;
		const event = JSON.parse(String(message.data)) as { name: string; data: unknown };
		if (event.name === "stream.ready") readyFrame = event.data;
		for (const listener of listeners.get(event.name) ?? []) {
			try {
				listener(event.data);
			} catch {
				// One plugin's listener failing leaves the others be.
			}
		}
	});
	opened.addEventListener("error", () => {
		if (source !== opened) return;
		opened.close();
		source = undefined;
		readyFrame = undefined;
		for (const listener of connectionListeners) listener(false);
		retry = setTimeout(() => {
			retry = undefined;
			connect();
		}, retryDelay(failures++));
	});
}

export function onEventConnection(listener: ConnectionListener): () => void {
	connectionListeners.add(listener);
	return () => void connectionListeners.delete(listener);
}

/** Calls `listener` with every `name` event from now on; answers what stops it. */
export function onEvent(name: string, listener: Listener): () => void {
	const set = listeners.get(name) ?? new Set();
	set.add(listener);
	listeners.set(name, set);
	if (name === "stream.ready" && readyFrame !== undefined) listener(readyFrame);
	connect();
	return () => {
		set.delete(listener);
		if (set.size === 0) listeners.delete(name);
		if (listeners.size === 0) {
			clearTimeout(retry);
			retry = undefined;
			source?.close();
			source = undefined;
			readyFrame = undefined;
			failures = 0;
			stopResume?.();
			stopResume = undefined;
		}
	};
}
