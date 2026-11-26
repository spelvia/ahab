/**
 * Vim's state as the status bar shows it, like Vim's own status line: the
 * mode (`-- INSERT --`) and the keys of a command not finished yet (`d2`,
 * Vim's `showcmd`). The editor with the focus publishes it; nothing here
 * loads Vim itself.
 */

import { useSyncExternalStore } from "react";

import { t } from "@/shared/i18n";

export type VimMode = "normal" | "insert" | "visual" | "visualLine" | "visualBlock" | "replace";

/** Vim's mode from its `vim-mode-change` event (`{ mode: "visual", subMode: "linewise" }`). */
export function vimModeOf(event: { mode?: string; subMode?: string }): VimMode {
	switch (event.mode) {
		case "insert":
			return "insert";
		case "replace":
			return "replace";
		case "visual":
			return event.subMode === "linewise"
				? "visualLine"
				: event.subMode === "blockwise"
					? "visualBlock"
					: "visual";
		default:
			return "normal";
	}
}

/** `NORMAL`, `INSERT`, `VISUAL LINE`…, as Vim's status line names the modes. */
export function vimModeLabel(mode: VimMode): string {
	return t(`editor.vim.modes.${mode}`);
}

/** What of Vim's key handling is pending: a count, a register, an operator, a prefix. */
interface PendingInput {
	prefixRepeat?: readonly string[];
	motionRepeat?: readonly string[];
	operator?: string | null;
	keyBuffer?: readonly string[];
	registerName?: string | null;
}

/**
 * The keys of the command being typed (`d2`, `"a`, `g`), as Vim's
 * `showcmd` shows them: the keys typed since the last command (`typed`),
 * while something is still pending; nothing in insert or replace mode,
 * where keys are text.
 */
export function pendingKeys(mode: VimMode, typed: string, input: PendingInput | undefined): string {
	if (mode === "insert" || mode === "replace" || !input) return "";
	const pending =
		Boolean(input.operator) ||
		Boolean(input.registerName) ||
		Boolean(input.prefixRepeat?.length) ||
		Boolean(input.motionRepeat?.length) ||
		Boolean(input.keyBuffer?.length);
	// Long runs are cut from the left, as Vim's showcmd column is.
	return pending ? [...typed].slice(-10).join("") : "";
}

export interface VimStatus {
	mode: VimMode;
	pending: string;
}

// --- The focused editor's Vim -------------------------------------------------

let shown: { owner: object; status: VimStatus } | null = null;
const listeners = new Set<() => void>();

function changed() {
	for (const listener of listeners) listener();
}

/**
 * An editor's Vim status, from the editor that has (or last had) the
 * focus; null when its Vim is off or it is gone.
 */
export function publishVimStatus(owner: object, status: VimStatus | null) {
	if (status) {
		const same =
			shown?.owner === owner &&
			shown.status.mode === status.mode &&
			shown.status.pending === status.pending;
		if (same) return;
		shown = { owner, status };
	} else {
		if (shown?.owner !== owner) return;
		shown = null;
	}
	changed();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

const current = () => shown?.status ?? null;

/** The focused editor's Vim status, or null when it has no Vim. */
export function useVimStatus(): VimStatus | null {
	return useSyncExternalStore(subscribe, current, current);
}
