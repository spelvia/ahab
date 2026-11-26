/** The workbench as plugins act on it, and the tabs of plugin windows. */

import { isDeclaration, type Language, resolveLanguage } from "@/shared/syntax/model";

/** What plugins act on in the workbench: set by the workbench as it changes. */
export interface Host {
	/** The current project, if any. */
	project?: { id: string; machineId: string; name: string; path: string };
	/** Opens (or brings forward) a window in the tiles. */
	openWindow: (tab: { id: string; component: string; title: string; params: unknown }) => void;
	/** Opens a session's window (a terminal a plugin started). */
	openSession: (session: { id: string; title: string }) => void;
	/** Shows a sidebar view by its key. */
	showView: (key: string) => void;
}

const noHost: Host = { openWindow: () => {}, openSession: () => {}, showView: () => {} };
let host: Host = noHost;
const hostListeners = new Set<() => void>();

export function currentHost(): Host {
	return host;
}

export function setHost(next: Host) {
	host = next;
	for (const listener of hostListeners) listener();
}

export function subscribeHost(listener: () => void): () => void {
	hostListeners.add(listener);
	return () => hostListeners.delete(listener);
}

/** The tab ID of a plugin window: one per kind when `single`. */
export function windowTabId(plugin: string, kind: string, single: boolean | undefined): string {
	return single ? `plugin:${plugin}:${kind}` : `plugin:${plugin}:${kind}:${Date.now()}`;
}

/**
 * The languages a plugin's manifest declares (`contributes.languages`),
 * their files served from the plugin's installed copy. Declarations
 * that do not read as one are left out; the server has checked the rest.
 */
export function contributedLanguagesOf(
	plugin: string,
	digest: string,
	manifest: unknown,
): Language[] {
	const declared = (manifest as { contributes?: { languages?: unknown } } | null)?.contributes
		?.languages;
	if (!Array.isArray(declared)) return [];
	const file = (path: string) =>
		`/api/v1/plugins/${encodeURIComponent(plugin)}/files/${path}?v=${encodeURIComponent(digest)}`;
	return declared
		.filter(isDeclaration)
		.map((declaration) => resolveLanguage(declaration, "", plugin, file));
}
