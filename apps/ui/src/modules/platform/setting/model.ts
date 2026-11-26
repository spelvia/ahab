/**
 * The editor's settings (Settings → Editor) as the server keeps
 * them, read into what the editor and file windows use. Values the server
 * does not send (an older one) take their defaults.
 */

import { DEFAULT_SETTINGS, type EditorSettings } from "@/shared/editor/model";

type AutoSave = "off" | "afterDelay" | "onFocusChange";

/** What file windows do besides the editor's look. */
export interface EditorBehaviour {
	/** Files open editable instead of read-only (per project too). */
	editableByDefault: boolean;
	autoSave: AutoSave;
	/** Milliseconds after typing stops, for `afterDelay`. */
	autoSaveDelay: number;
}

const DEFAULT_BEHAVIOUR: EditorBehaviour = {
	editableByDefault: false,
	autoSave: "off",
	autoSaveDelay: 1000,
};

function number(value: unknown, fallback: number): number {
	return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function flag(value: unknown, fallback: boolean): boolean {
	return typeof value === "boolean" ? value : fallback;
}

/** The editor's look, from the section's values. */
export function editorSettings(values: Record<string, unknown> | undefined): EditorSettings {
	const v = values ?? {};
	return {
		fontSize: number(v.fontSize, DEFAULT_SETTINGS.fontSize),
		lineWrapping: flag(v.lineWrapping, DEFAULT_SETTINGS.lineWrapping),
		lineNumbers: flag(v.lineNumbers, DEFAULT_SETTINGS.lineNumbers),
		indentationGuides: flag(v.indentationGuides, DEFAULT_SETTINGS.indentationGuides),
		tabSize: number(v.tabSize, DEFAULT_SETTINGS.tabSize),
		vim: flag(v.vim, DEFAULT_SETTINGS.vim),
		problemsAtLineEnd: flag(v.problemsAtLineEnd, false),
	};
}

/** How a message sent while the agent works reaches it. */
export type ChatDelivery = "queue" | "interrupt";

/** The editor section's fields that belong to the chat composer (Settings → Agent editor). */
export const CHAT_FIELDS = ["chatWhileWorking", "chatVim"] as const;

/** The chat composer's settings, kept in the editor's section. */
export interface ChatSettings {
	/** What Enter does while the agent works (the composer can choose otherwise). */
	whileWorking: ChatDelivery;
	/** Vim key bindings in the composer. */
	vim: boolean;
}

/** The chat composer's settings, from the editor section's values. */
export function chatSettings(values: Record<string, unknown> | undefined): ChatSettings {
	const v = values ?? {};
	return {
		whileWorking: v.chatWhileWorking === "interrupt" ? "interrupt" : "queue",
		vim: flag(v.chatVim, false),
	};
}

/** What file windows do, from the section's values. */
export function editorBehaviour(values: Record<string, unknown> | undefined): EditorBehaviour {
	const v = values ?? {};
	const autoSave = ["off", "afterDelay", "onFocusChange"].includes(String(v.autoSave))
		? (v.autoSave as AutoSave)
		: DEFAULT_BEHAVIOUR.autoSave;
	return {
		editableByDefault: flag(v.editableByDefault, DEFAULT_BEHAVIOUR.editableByDefault),
		autoSave,
		autoSaveDelay: number(v.autoSaveDelay, DEFAULT_BEHAVIOUR.autoSaveDelay),
	};
}
