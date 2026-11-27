/**
 * What plugins add to the web app: views, windows, new-window
 * entries, palette commands, status bar items and settings pages, kept by
 * the plugin that added them so a plugin's cleanup removes them all.
 */
import type { ComponentType, ReactNode } from "react";

/** A sidebar view, or an activity bar icon that opens a window. */
export interface PluginView {
	/** `plugin:<plugin>:<id>`, as the sidebars keep it. */
	key: string;
	plugin: string;
	id: string;
	title: string;
	icon: ReactNode;
	/** What the view shows; absent when the icon opens a window instead. */
	Component?: ComponentType;
	/** A window kind (of this plugin) the icon opens or brings forward. */
	opens?: { window: string; params?: unknown };
}

/** A kind of window a plugin adds to the tiles. */
interface PluginWindow {
	plugin: string;
	kind: string;
	title: (params: unknown) => string;
	/** One per kind: opening it again brings it forward. */
	single?: boolean;
	Component: ComponentType<{ params: unknown }>;
}

/** An entry in the + menus or the palette. */
interface PluginItem {
	key: string;
	plugin: string;
	id: string;
	title: string;
	icon?: ReactNode;
	keywords?: string[];
	run: () => void;
}

/** A small item in the status bar. */
interface PluginStatusItem {
	key: string;
	plugin: string;
	id: string;
	Component: ComponentType;
}

/** A plugin's own settings page. */
interface PluginSettingsPage {
	plugin: string;
	Component: ComponentType;
}

export interface Contributions {
	views: PluginView[];
	windows: PluginWindow[];
	newWindow: PluginItem[];
	palette: PluginItem[];
	status: PluginStatusItem[];
	settings: PluginSettingsPage[];
	/** Plugins whose web entry failed, and why. */
	failures: Record<string, string>;
}

type ListKey = Exclude<keyof Contributions, "failures">;

const empty: Contributions = {
	views: [],
	windows: [],
	newWindow: [],
	palette: [],
	status: [],
	settings: [],
	failures: {},
};

/** The contributions, replaced as a whole on every change. */
let current: Contributions = empty;
const listeners = new Set<() => void>();

function set(next: Contributions) {
	current = next;
	for (const listener of listeners) listener();
}

export function contributions(): Contributions {
	return current;
}

export function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** Adds an item to a list; answers what removes it. */
export function add<K extends ListKey>(list: K, item: Contributions[K][number]): () => void {
	set({ ...current, [list]: [...current[list], item] });
	return () => set({ ...current, [list]: (current[list] as unknown[]).filter((x) => x !== item) });
}

/** Removes everything a plugin added. */
export function removePlugin(plugin: string) {
	const without = <T extends { plugin: string }>(items: T[]) =>
		items.filter((item) => item.plugin !== plugin);
	const failures = { ...current.failures };
	delete failures[plugin];
	set({
		views: without(current.views),
		windows: without(current.windows),
		newWindow: without(current.newWindow),
		palette: without(current.palette),
		status: without(current.status),
		settings: without(current.settings),
		failures,
	});
}

/** Records that a plugin's web entry failed. */
export function fail(plugin: string, message: string) {
	set({ ...current, failures: { ...current.failures, [plugin]: message } });
}

/** For tests: back to nothing. */
export function reset() {
	set(empty);
}
