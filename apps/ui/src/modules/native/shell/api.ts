/**
 * The built-in screens' calls into the app (`apps/native/src/commands.rs`):
 * the only code that pairs, starts, stops, joins or leaves.
 */
import { invoke } from "@tauri-apps/api/core";

export interface Server {
	id: string;
	name: string;
	url: string;
	local: boolean;
	deviceId?: string | null;
	lastPath?: string | null;
}

export interface Overview {
	servers: Server[];
	last?: string | null;
	store: "keychain" | "encryptedFile";
	storeNote?: string | null;
}

type Role = "server" | "machine" | "none";

interface Binary {
	path: string;
	version?: string | null;
}

export interface ThisComputer {
	role: Role;
	installed?: Binary | null;
	bundled?: Binary | null;
	serviceManager?: string | null;
	managed: boolean;
	serviceInstalled: boolean;
	running: boolean;
	url?: string | null;
	serverVersion?: string | null;
	joined?: string | null;
	machine?: { name?: string; state?: string; note?: string } | null;
	mode?: string | null;
}

export interface Settings {
	keepInTray?: boolean | null;
	globalShortcut?: string | null;
	notifications: boolean;
	checkForUpdates: boolean;
}

export interface Boot {
	servers: Overview;
	computer: ThisComputer;
	settings: Settings;
	version: string;
}

export interface Reached {
	url: string;
	version: string;
	known?: Server | null;
}

export type Waited =
	| { state: "waiting" }
	| { state: "denied" }
	| { state: "expired" }
	| { state: "paired"; server: Server };

export type Link =
	| { kind: "pair"; server: string; secret?: string; code?: string }
	| { kind: "join"; server: string; code: string; fingerprint?: string }
	| { kind: "open"; server: string; path: string };

export interface Updates {
	enabled: boolean;
	available: boolean;
	note: string;
}

/** What a failed call says: a stable code and words for the person. */
/** This computer's machine and the agents it found (the start checklist). */
export interface LocalAgents {
	machine?: string | null;
	online: boolean;
	agents: { agent: string; label: string; state: string }[];
}

export interface Failure {
	code: string;
	message: string;
}

export function failure(error: unknown): Failure {
	if (error && typeof error === "object" && "message" in error) {
		const { code, message } = error as { code?: unknown; message?: unknown };
		return { code: typeof code === "string" ? code : "unexpected", message: String(message) };
	}
	return { code: "unexpected", message: String(error) };
}

export const native = {
	boot: () => invoke<Boot>("boot"),
	reach: (address: string) => invoke<Reached>("reach", { address }),
	pair: (url: string, secret?: string, code?: string) =>
		invoke<Server>("pair", { url, secret, code }),
	ask: (url: string) => invoke<{ id: string; code: string }>("ask", { url }),
	wait: (id: string) => invoke<Waited>("wait", { id }),
	openServer: (id: string, path?: string) => invoke<void>("open_server", { id, path }),
	openServerWindow: (id: string) => invoke<void>("open_server_window", { id }),
	removeServer: (id: string) => invoke<Server>("remove_server", { id }),
	renameServer: (id: string, name: string) => invoke<void>("rename_server", { id, name }),
	thisComputer: () => invoke<ThisComputer>("this_computer"),
	runHere: () => invoke<Server>("run_here"),
	existing: () => invoke<Server>("use_existing"),
	agentsHere: (id: string) => invoke<LocalAgents>("agents_here", { id }),
	openInBrowser: (url: string) => invoke<void>("open_in_browser", { url }),
	startHere: () => invoke<ThisComputer>("start_here"),
	stopHere: () => invoke<ThisComputer>("stop_here"),
	joinServer: (id: string) => invoke<ThisComputer>("join_server", { id }),
	joinWithCode: (server: string, code: string, fingerprint?: string) =>
		invoke<ThisComputer>("join_with_code", { server, code, fingerprint }),
	leave: () => invoke<ThisComputer>("leave"),
	parseLink: (link: string) => invoke<Link>("parse_link", { link }),
	takeLink: () => invoke<string | null>("take_link"),
	getSettings: () => invoke<Settings>("get_settings"),
	setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
	checkForUpdates: () => invoke<Updates>("check_for_updates"),
	checkServer: (id: string) =>
		invoke<{ reachable: boolean; paired: boolean; message?: string }>("check_server", { id }),
	quit: () => invoke<void>("quit"),
};
