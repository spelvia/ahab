import { useSyncExternalStore } from "react";

/** A built-in screen's address: `#/connect?machine=1`. */
export interface Route {
	path: string;
	params: URLSearchParams;
}

/**
 * Whether this page is the desktop app's own copy of the web app:
 * opened by the app (`__AHAB_APP__`, set before any script runs) from its
 * own origin, never from a server's. Only here are the built-in screens
 * shown, and only here does the app allow its privileged commands.
 */
export function isBuiltinScreen(): boolean {
	if (!("__AHAB_APP__" in window)) return false;
	const { protocol, hostname } = window.location;
	if (protocol === "tauri:" || hostname === "tauri.localhost") return true;
	// `make app-dev`: the app shows the web app's dev server as its own.
	return import.meta.env.DEV && hostname === "localhost";
}

/**
 * Whether this is the app on a phone (Android now): a client only,
 * which never runs Ahab itself. Its web view says so in its user agent.
 */
export function isPhoneApp(): boolean {
	return isBuiltinScreen() && /Android|iPhone|iPad/i.test(window.navigator.userAgent);
}

function read(): Route {
	const hash = window.location.hash.replace(/^#/, "") || "/start";
	const [path = "/start", query = ""] = hash.split("?");
	return { path, params: new URLSearchParams(query) };
}

let current: Route | undefined;

function subscribe(listener: () => void) {
	const changed = () => {
		current = read();
		listener();
	};
	window.addEventListener("hashchange", changed);
	return () => window.removeEventListener("hashchange", changed);
}

/** The built-in screen the address names. */
export function useRoute(): Route {
	return useSyncExternalStore(subscribe, () => (current ??= read()));
}

/** Goes to a built-in screen. */
export function go(path: string, params?: Record<string, string | undefined>) {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params ?? {})) if (value) query.set(key, value);
	const text = query.toString();
	window.location.hash = text ? `${path}?${text}` : path;
}
