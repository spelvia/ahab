/**
 * The desktop app's native bridge, when the web app runs inside
 * Ahab for desktop: a few functions the app offers to its paired server's
 * pages, and nothing else. In a browser there is no bridge and every
 * function here is a no-op that says so, so the web app works unchanged.
 */

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

interface TauriInternals {
	invoke?: Invoke;
}

/** Where Tauri puts its IPC in the page. */
const INTERNALS = "__TAURI_INTERNALS__";

/** The app's invoke, captured once: later scripts cannot swap it. */
const invoke: Invoke | undefined = (() => {
	if (typeof window === "undefined") return undefined;
	const internals = Reflect.get(window, INTERNALS) as TauriInternals | undefined;
	return typeof internals?.invoke === "function" ? internals.invoke.bind(internals) : undefined;
})();

/** A server in the app's list. */
export interface AppServer {
	id: string;
	name: string;
	url: string;
	local: boolean;
	current: boolean;
}

/** What `app.info()` says. */
export interface AppInfo {
	app: true;
	version: string;
	platform: string;
	server: { id: string; name: string; url: string; local: boolean };
	functions: string[];
}

/** A native notification and the page it opens. */
export interface AppNotification {
	title: string;
	body?: string;
	/** A path of this server, opened when the notification is clicked. */
	target?: string;
}

/** The phone's background connection, as `app.background` answers. */
export interface BackgroundStatus {
	permission: "granted" | "denied" | "prompt";
	stayConnected: boolean;
	running: boolean;
}

/** What `app.background` is asked. */
export type BackgroundAsk =
	| { action: "status" }
	| { action: "allow" }
	| { action: "working"; working: number }
	| { action: "stay"; stay: boolean }
	| { action: "battery" };

let detected: Promise<AppInfo | undefined> | undefined;

/** The app's answer, or undefined in a browser (or a page the app does not let in). */
function info(): Promise<AppInfo | undefined> {
	detected ??= invoke
		? (invoke("bridge_info") as Promise<AppInfo>).catch(() => undefined)
		: Promise.resolve(undefined);
	return detected;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T | undefined> {
	if (!invoke || !(await info())) return undefined;
	return (await invoke(command, args)) as T;
}

/** The bridge's functions. */
export const app = {
	info,
	notify: (notification: AppNotification) => call<void>("bridge_notify", { notification }),
	badge: (count: number) => call<void>("bridge_badge", { count: Math.max(0, Math.round(count)) }),
	openExternal: (url: string) => call<void>("bridge_open_external", { url }),
	servers: async () => (await call<AppServer[]>("bridge_servers")) ?? [],
	switchServer: (id: string) => call<void>("bridge_switch_server", { id }),
	addServer: () => call<void>("bridge_add_server"),
	newWindow: (path?: string) => call<void>("bridge_new_window", { path }),
	/** The local role and service state; read only. */
	thisComputer: () => call<unknown>("bridge_this_computer"),
	/** The phone app's background connection; only on Android (`info().platform`). */
	background: (ask: BackgroundAsk) => call<BackgroundStatus>("bridge_background", { ask }),
};
