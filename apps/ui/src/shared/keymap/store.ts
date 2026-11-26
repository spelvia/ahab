/**
 * The keymap in use: Ahab's shortcuts with this browser's changes, kept in
 * the preferences and shared by everything that reads keys.
 */

import { type ActionId, keymapOf, type Keymap } from "@/shared/keymap/model";
import { rememberShortcuts, shortcuts } from "@/shared/preferences/store";

let changed = shortcuts();
let current = keymapOf(changed);
const listeners = new Set<() => void>();

/** The keymap now, for code outside React (the terminal). */
export function keymap(): Keymap {
	return current;
}

/** The actions whose keys were changed here, with their keys. */
export function changedShortcuts(): Record<string, string[]> {
	return changed;
}

/** Gives `id` these keys (resolved, `Ctrl+KeyK`), or Ahab's own with none given. */
export function setShortcut(id: ActionId, keys: string[] | undefined) {
	const next = { ...changed };
	if (keys) next[id] = keys;
	else delete next[id];
	changed = next;
	current = keymapOf(changed);
	rememberShortcuts(changed);
	for (const listener of listeners) listener();
}

/** Every action back to Ahab's keys. */
export function resetShortcuts() {
	changed = {};
	current = keymapOf(changed);
	rememberShortcuts(changed);
	for (const listener of listeners) listener();
}

/** Calls `listener` on every change; answers how to stop. */
export function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}
