/**
 * Vim key mappings of this device (Settings → Keyboard): `jk` → `<Esc>` in
 * insert mode, as `:imap jk <Esc>` makes. Kept in the preferences beside
 * the shortcuts; the editor applies them with Vim's own `map` and `noremap`.
 */

import { useSyncExternalStore } from "react";

import { rememberVimMappings, type VimMapping, vimMappings } from "@/shared/preferences/store";

export type { VimMapping };

export const VIM_MAPPING_MODES: readonly VimMapping["mode"][] = [
	"normal",
	"insert",
	"visual",
	"all",
];

/** Vim's names of special keys, as the editor's Vim writes them, by their lowercase spellings. */
const NAMES: Record<string, string> = {
	esc: "Esc",
	escape: "Esc",
	cr: "CR",
	enter: "CR",
	return: "CR",
	bs: "BS",
	backspace: "BS",
	del: "Del",
	delete: "Del",
	ins: "Ins",
	insert: "Ins",
	space: "Space",
	tab: "Tab",
	up: "Up",
	down: "Down",
	left: "Left",
	right: "Right",
	home: "Home",
	end: "End",
	pageup: "PageUp",
	pagedown: "PageDown",
	lt: "lt",
};
for (let n = 1; n <= 12; n++) NAMES[`f${n}`] = `F${n}`;

/** Modifiers in the order the editor's Vim writes them: `<C-A-M-S-x>`. */
const MODIFIERS = ["C", "A", "M", "S"] as const;

/**
 * Keys in Vim's notation, written as the editor's Vim reads them (`<c-s>`
 * becomes `<C-s>`, `<Enter>` becomes `<CR>`), or the first key it does not
 * know (`<Leader>`, which the editor's Vim does not have).
 */
export function normalizeVimKeys(text: string): { keys: string } | { unknown: string } {
	let keys = "";
	for (const [token] of text.matchAll(/<[^<>\s]+>|[\s\S]/gu)) {
		if (token.length < 3 || !token.startsWith("<")) {
			keys += token;
			continue;
		}
		const parts = token.slice(1, -1).split("-");
		// `<C-->`: the key is a minus.
		const key = parts.at(-1) === "" && parts.length > 1 ? "-" : (parts.pop() ?? "");
		if (key === "-") parts.splice(-2, 2);
		const held = new Set(parts.map((part) => part.toUpperCase()));
		const name = key.length === 1 && held.size ? key : NAMES[key.toLowerCase()];
		if (!name || [...held].some((part) => !(MODIFIERS as readonly string[]).includes(part)))
			return { unknown: token };
		keys += `<${MODIFIERS.filter((modifier) => held.has(modifier))
			.map((modifier) => `${modifier}-`)
			.join("")}${name}>`;
	}
	return { keys };
}

/** What is wrong with a mapping, if anything. */
export type VimMappingProblem =
	| { problem: "lhsEmpty" | "rhsEmpty" | "exCommand" | "typing" | "same" | "duplicate" }
	| { problem: "unknownKey"; key: string };

/**
 * Whether a mapping can be applied, among `others` (the rest of the list):
 * keys typed and stood for, in a notation Vim reads; not an Ex command's
 * name (`:w`); not a lone character typed in insert mode (it could no
 * longer be typed); not mapped to itself; not a second mapping of the same
 * keys in the same mode.
 */
export function vimMappingProblem(
	mapping: VimMapping,
	others: readonly VimMapping[],
): VimMappingProblem | undefined {
	if (!mapping.lhs.trim()) return { problem: "lhsEmpty" };
	if (!mapping.rhs.trim()) return { problem: "rhsEmpty" };
	if (mapping.lhs.startsWith(":")) return { problem: "exCommand" };
	const lhs = normalizeVimKeys(mapping.lhs.trim());
	if ("unknown" in lhs) return { problem: "unknownKey", key: lhs.unknown };
	const rhs = normalizeVimKeys(mapping.rhs.trim());
	if ("unknown" in rhs) return { problem: "unknownKey", key: rhs.unknown };
	if (mapping.mode === "insert" && [...lhs.keys].length === 1) return { problem: "typing" };
	if (lhs.keys === rhs.keys) return { problem: "same" };
	const same = others.some((other) => {
		const keys = normalizeVimKeys(other.lhs.trim());
		return other.mode === mapping.mode && "keys" in keys && keys.keys === lhs.keys;
	});
	return same ? { problem: "duplicate" } : undefined;
}

/** A mapping as the editor's Vim takes it, or null when it cannot be applied. */
export function appliedVimMapping(
	mapping: VimMapping,
	others: readonly VimMapping[],
): VimMapping | null {
	if (vimMappingProblem(mapping, others)) return null;
	const lhs = normalizeVimKeys(mapping.lhs.trim());
	const rhs = normalizeVimKeys(mapping.rhs.trim());
	if (!("keys" in lhs) || !("keys" in rhs)) return null;
	return { ...mapping, lhs: lhs.keys, rhs: rhs.keys };
}

// --- The mappings in use --------------------------------------------------------

let current = vimMappings();
const listeners = new Set<() => void>();

/** This device's mappings, for code outside React (the editor). */
export function currentVimMappings(): VimMapping[] {
	return current;
}

/** Keeps the mappings and tells every editor. */
export function setVimMappings(mappings: VimMapping[]) {
	current = mappings;
	rememberVimMappings(mappings);
	for (const listener of listeners) listener();
}

/** Calls `listener` on every change; answers how to stop. */
export function subscribeVimMappings(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

/** This device's mappings, kept up to date. */
export function useVimMappings(): VimMapping[] {
	return useSyncExternalStore(subscribeVimMappings, currentVimMappings, currentVimMappings);
}
