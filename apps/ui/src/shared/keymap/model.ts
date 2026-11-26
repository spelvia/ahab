/**
 * Ahab's shortcuts: every action a key can ask for, the keys each has by
 * default, and how a key press, a stored key and a shown key relate. The
 * key handler, the hints in menus and the terminal's pass-through all read
 * this one list.
 *
 * Keys are written `Ctrl+Alt+Shift+Meta+<code>`, the modifiers in that
 * order, with the key's `KeyboardEvent.code` (`KeyK`, `Backquote`), so they
 * mean the same key on every keyboard layout. Defaults use two stand-ins:
 * `Mod` (⌘ on a Mac, Ctrl elsewhere) and `Win`, the modifier a browser tab
 * leaves free for window keys (⌃ on a Mac, Alt elsewhere): browsers keep
 * ⌘/Ctrl+T, W, N and Tab for their own tabs, and pass them to Ahab only
 * when it is installed as an app.
 */

import { t } from "@/shared/i18n";
import { mac } from "@/shared/platform";

/** Which core feature an action belongs to: off with it. */
type ActionFeature = "terminal" | "files" | "search";

export interface Action {
	id: ActionId;
	title: string;
	/** Which group it is listed in. */
	group: "General" | "Windows" | "Files" | "Editor" | "Chat";
	/** Ahab's keys for it, `Mod` and `Win` standing for the platform's. */
	defaults: string[];
	feature?: ActionFeature;
	/** Its keys work even as plain Ctrl+letter in a terminal (Ctrl+P), which otherwise stay with the shell. */
	overShell?: boolean;
}

export type ActionId =
	| "palette"
	| "sidebar.left"
	| "sidebar.right"
	| "session.new"
	| "terminal.new"
	| "window.close"
	| "window.reopen"
	| "window.next"
	| "window.previous"
	| "window.split"
	| "files.quickOpen"
	| "search.find"
	| "panel.toggle"
	| "panel.problems"
	| "panel.output"
	| ProjectActionId
	| EditorActionId
	| ChatActionId;

/** A chat's keys: they work while one of its requests has the focus. */
export type ChatActionId = "chat.allow" | "chat.deny";

/** Switching to the project listed first, second… ninth (Settings → Projects orders them). */
type ProjectActionId = `project.${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;

const PROJECT_NUMBERS = [1, 2, 3, 4, 5, 6, 7, 8, 9] as const;

/**
 * The editor's own commands: their keys work while an editor has
 * the focus, after Vim's when Vim is on.
 */
export type EditorActionId =
	| "editor.save"
	| "editor.toggleEdit"
	| "editor.find"
	| "editor.goToLine"
	| "editor.selectNextOccurrence"
	| "editor.deleteLine"
	| "editor.moveLineUp"
	| "editor.moveLineDown"
	| "editor.copyLineUp"
	| "editor.copyLineDown"
	| "editor.toggleComment"
	| "editor.goToDefinition"
	| "editor.goToTypeDefinition"
	| "editor.goToImplementation"
	| "editor.goToReferences"
	| "editor.showHover"
	| "editor.rename"
	| "editor.codeActions"
	| "editor.format"
	| "editor.nextProblem"
	| "editor.previousProblem";

const LIST: readonly Omit<Action, "title">[] = [
	{ id: "palette", group: "General", defaults: ["Mod+KeyK"] },
	{
		id: "sidebar.left",
		group: "General",
		defaults: ["Mod+KeyB"],
	},
	{
		id: "sidebar.right",
		group: "General",
		defaults: ["Alt+Mod+KeyB"],
	},
	{
		id: "panel.toggle",
		group: "General",
		defaults: ["Mod+KeyJ"],
	},
	{
		id: "panel.problems",
		group: "General",
		defaults: ["Mod+Shift+KeyM"],
	},
	{
		id: "panel.output",
		group: "General",
		defaults: ["Mod+Shift+KeyU"],
	},
	// Browsers keep Ctrl/⌘+1…9 for their own tabs: Alt (⌥) works everywhere.
	...PROJECT_NUMBERS.map((number): Omit<Action, "title"> => ({
		id: `project.${number}`,
		group: "General",
		defaults: [`Alt+Digit${number}`],
	})),
	{
		id: "session.new",
		group: "Windows",
		defaults: ["Win+KeyT", "Mod+KeyT", "Mod+KeyN"],
	},
	{
		id: "terminal.new",
		group: "Windows",
		defaults: ["Win+Backquote", "Mod+Backquote", "Ctrl+Backquote"],
		feature: "terminal",
	},
	{
		id: "window.close",
		group: "Windows",
		defaults: ["Win+KeyW", "Mod+KeyW"],
	},
	{
		id: "window.reopen",
		group: "Windows",
		defaults: ["Win+Shift+KeyT", "Mod+Shift+KeyT"], // gitleaks:allow (a key, not a secret)
	},
	{
		id: "window.next",
		group: "Windows",
		defaults: ["Win+BracketRight", "Ctrl+Tab", "Ctrl+PageDown"],
	},
	{
		id: "window.previous",
		group: "Windows",
		defaults: ["Win+BracketLeft", "Ctrl+Shift+Tab", "Ctrl+PageUp"],
	},
	{
		id: "window.split",
		group: "Windows",
		defaults: ["Win+Backslash"],
	},
	{
		id: "files.quickOpen",
		group: "Files",
		defaults: ["Mod+KeyP"],
		feature: "files",
		overShell: true,
	},
	{
		id: "search.find",
		group: "Files",
		defaults: ["Mod+Shift+KeyF"],
		feature: "search",
	},
	editorAction("editor.save", ["Mod+KeyS"]),
	editorAction("editor.toggleEdit", ["Mod+KeyE"]),
	editorAction("editor.find", ["Mod+KeyF"]),
	editorAction("editor.goToLine", ["Ctrl+KeyG"]),
	editorAction("editor.selectNextOccurrence", ["Mod+KeyD"]),
	editorAction("editor.deleteLine", ["Mod+Shift+KeyK"]),
	editorAction("editor.moveLineUp", ["Alt+ArrowUp"]),
	editorAction("editor.moveLineDown", ["Alt+ArrowDown"]),
	editorAction("editor.copyLineUp", ["Alt+Shift+ArrowUp"]),
	editorAction("editor.copyLineDown", ["Alt+Shift+ArrowDown"]),
	editorAction("editor.toggleComment", ["Mod+Slash"]),
	editorAction("editor.goToDefinition", ["F12"]),
	editorAction("editor.goToTypeDefinition", []),
	editorAction("editor.goToImplementation", ["Mod+F12"]),
	editorAction("editor.goToReferences", ["Shift+F12"]),
	editorAction("editor.showHover", []),
	editorAction("editor.rename", ["F2"]),
	editorAction("editor.codeActions", ["Mod+Period"]),
	editorAction("editor.format", ["Alt+Shift+KeyF"]),
	editorAction("editor.nextProblem", ["F8"]),
	editorAction("editor.previousProblem", ["Shift+F8"]),
	{ id: "chat.allow", group: "Chat", defaults: ["Enter"] },
	{ id: "chat.deny", group: "Chat", defaults: ["Escape"] },
];

function editorAction(id: EditorActionId, defaults: string[]): Omit<Action, "title"> {
	return { id, group: "Editor", defaults, feature: "files" };
}

/** Each action's title's key. */
const TITLES = {
	palette: "keymap.actions.palette",
	"sidebar.left": "keymap.actions.leftSidebar",
	"sidebar.right": "keymap.actions.rightSidebar",
	"panel.toggle": "keymap.actions.togglePanel",
	"panel.problems": "keymap.actions.showProblems",
	"panel.output": "keymap.actions.showOutput",
	...(Object.fromEntries(
		PROJECT_NUMBERS.map((number) => [`project.${number}`, "keymap.actions.switchProject"]),
	) as Record<ProjectActionId, "keymap.actions.switchProject">),
	"session.new": "keymap.actions.newSession",
	"terminal.new": "keymap.actions.newTerminal",
	"window.close": "keymap.actions.closeWindow",
	"window.reopen": "keymap.actions.reopenWindow",
	"window.next": "keymap.actions.nextWindow",
	"window.previous": "keymap.actions.previousWindow",
	"window.split": "keymap.actions.splitWindow",
	"files.quickOpen": "keymap.actions.goToFile",
	"search.find": "keymap.actions.findInFiles",
	"editor.save": "keymap.actions.editor.save",
	"editor.toggleEdit": "keymap.actions.editor.toggleEdit",
	"editor.find": "keymap.actions.editor.find",
	"editor.goToLine": "keymap.actions.editor.goToLine",
	"editor.selectNextOccurrence": "keymap.actions.editor.selectNextOccurrence",
	"editor.deleteLine": "keymap.actions.editor.deleteLine",
	"editor.moveLineUp": "keymap.actions.editor.moveLineUp",
	"editor.moveLineDown": "keymap.actions.editor.moveLineDown",
	"editor.copyLineUp": "keymap.actions.editor.copyLineUp",
	"editor.copyLineDown": "keymap.actions.editor.copyLineDown",
	"editor.toggleComment": "keymap.actions.editor.toggleComment",
	"editor.goToDefinition": "keymap.actions.editor.goToDefinition",
	"editor.goToTypeDefinition": "keymap.actions.editor.goToTypeDefinition",
	"editor.goToImplementation": "keymap.actions.editor.goToImplementation",
	"editor.goToReferences": "keymap.actions.editor.goToReferences",
	"editor.showHover": "keymap.actions.editor.showHover",
	"editor.rename": "keymap.actions.editor.rename",
	"editor.codeActions": "keymap.actions.editor.codeActions",
	"editor.format": "keymap.actions.editor.format",
	"editor.nextProblem": "keymap.actions.editor.nextProblem",
	"editor.previousProblem": "keymap.actions.editor.previousProblem",
	"chat.allow": "keymap.actions.chat.allow",
	"chat.deny": "keymap.actions.chat.deny",
} as const satisfies Record<ActionId, string>;

/** Every action; its `title` is read in the app's language each time. */
export const ACTIONS: readonly Action[] = LIST.map(
	(action) =>
		Object.defineProperty(action, "title", {
			get: () => t(TITLES[action.id], { number: action.id.split(".")[1] }),
			enumerable: true,
		}) as Action,
);

const MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;

/** A stored key with `Mod` and `Win` made the platform's, in order: `Ctrl+Shift+KeyF`. */
export function resolve(key: string, onMac = mac): string {
	const parts = key.split("+");
	const code = parts.pop() ?? "";
	const held = new Set(
		parts.map((part) =>
			part === "Mod" ? (onMac ? "Meta" : "Ctrl") : part === "Win" ? (onMac ? "Ctrl" : "Alt") : part,
		),
	);
	return [...MODIFIERS.filter((modifier) => held.has(modifier)), code].join("+");
}

type KeyPress = Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "code">;

/** The key pressed, written as stored keys are; undefined for a modifier alone. */
export function keyOf(event: KeyPress): string | undefined {
	if (!event.code || /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(event.code))
		return undefined;
	const held = [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey];
	return [...MODIFIERS.filter((_, index) => held[index]), event.code].join("+");
}

/** Each action's keys, resolved: Ahab's, or the ones changed in their place. */
export type Keymap = Record<ActionId, string[]>;

export function keymapOf(changed: Record<string, string[]>, onMac = mac): Keymap {
	const map = {} as Keymap;
	for (const action of ACTIONS) {
		const keys = (changed[action.id] ?? action.defaults).map((key) => resolve(key, onMac));
		map[action.id] = [...new Set(keys)];
	}
	return map;
}

/** The action a key press asks for, if any (of those `allowed`). */
export function actionFor(
	map: Keymap,
	event: KeyPress,
	allowed: (action: Action) => boolean = () => true,
): Action | undefined {
	const key = keyOf(event);
	if (!key) return undefined;
	return ACTIONS.find((action) => map[action.id].includes(key) && allowed(action));
}

/** Whether an action is the editor's: its keys work only while an editor has the focus. */
function isEditorAction(action: Action): boolean {
	return action.group === "Editor";
}

/**
 * The editor's actions a key asks for (as `keyOf` writes it), in the order
 * they are tried: the first that applies wins, as in CodeMirror's keymaps
 * (F12 with no language tools goes on to whatever else has it).
 */
export function editorActionsFor(map: Keymap, key: string): EditorActionId[] {
	return ACTIONS.filter((action) => isEditorAction(action) && map[action.id].includes(key)).map(
		(action) => action.id as EditorActionId,
	);
}

/** The chat action a key press asks for while a request has the focus, if any. */
export function chatActionFor(map: Keymap, event: KeyPress): ChatActionId | undefined {
	const key = keyOf(event);
	if (!key) return undefined;
	return (["chat.allow", "chat.deny"] as const).find((id) => map[id].includes(key));
}

/** Keys given to more than one action, with the actions. */
export function conflicts(map: Keymap): Map<string, ActionId[]> {
	const byKey = new Map<string, ActionId[]>();
	for (const action of ACTIONS)
		for (const key of map[action.id]) byKey.set(key, [...(byKey.get(key) ?? []), action.id]);
	return new Map([...byKey].filter(([, actions]) => actions.length > 1));
}

const NAMES: Record<string, string> = {
	Backquote: "`",
	BracketLeft: "[",
	BracketRight: "]",
	Backslash: "\\",
	Slash: "/",
	Comma: ",",
	Period: ".",
	Semicolon: ";",
	Quote: "'",
	Minus: "-",
	Equal: "=",
	Escape: "Esc",
	ArrowUp: "↑",
	ArrowDown: "↓",
	ArrowLeft: "←",
	ArrowRight: "→",
	Enter: mac ? "↩" : "Enter",
	Backspace: mac ? "⌫" : "Backspace",
};

const MAC_SYMBOLS: Record<string, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Meta: "⌘" };

/** A key's parts as printed on keycaps: `["⌃", "⇧", "T"]` on a Mac, `["Ctrl", "Shift", "T"]` elsewhere. */
export function keyParts(key: string, onMac = mac): string[] {
	const parts = key.split("+");
	const code = parts.pop() ?? "";
	const name = NAMES[code] ?? code.replace(/^(Key|Digit|Numpad)/, "");
	return [...(onMac ? parts.map((part) => MAC_SYMBOLS[part] ?? part) : parts), name];
}

/** A key as people read it: `⌃⇧T` on a Mac, `Ctrl+Shift+T` elsewhere. */
export function shown(key: string, onMac = mac): string {
	return keyParts(key, onMac).join(onMac ? "" : "+");
}

/** Whether a key is safe to give an action: it holds Ctrl, Alt or ⌘ (or is a function key), so it never fires while typing. */
export function usable(key: string): boolean {
	return /(^|\+)(Ctrl|Alt|Meta)\+/.test(key) || /(^|\+)F([1-9]|1[0-9]|2[0-4])$/.test(key);
}

/** Keys a browser tab never gets: the browser keeps them for its own tabs. */
export function browserKeeps(key: string, onMac = mac): boolean {
	const mod = onMac ? "Meta" : "Ctrl";
	return (
		[`${mod}+KeyT`, `${mod}+KeyW`, `${mod}+KeyN`, `${mod}+Shift+KeyT`].includes(key) ||
		/^Ctrl\+(Shift\+)?(Tab|PageUp|PageDown)$/.test(key)
	);
}

/** The key to show for an action: its first that reaches Ahab here. */
export function hintFor(
	map: Keymap,
	id: ActionId,
	appWindow: boolean,
	onMac = mac,
): string | undefined {
	// An action this keymap does not know (from a newer or older client) has no key.
	const keys = map[id] ?? [];
	const key = appWindow ? keys[0] : (keys.find((each) => !browserKeeps(each, onMac)) ?? keys[0]);
	return key ? shown(key, onMac) : undefined;
}

/**
 * Whether a key pressed in a terminal is Ahab's rather than the shell's, as
 * VS Code's terminal decides. On a Mac every ⌘ key is Ahab's (⌃ keys, like
 * ⌃W, stay with the shell). Elsewhere a key given to an action is Ahab's,
 * unless it is plain Ctrl with a letter (Ctrl+K, Ctrl+W), which the shell
 * keeps, except for actions that take them (Ctrl+P, go to file).
 */
export function forAhab(map: Keymap, event: KeyPress, onMac = mac): boolean {
	if (onMac) return event.metaKey;
	// The editor's and a chat's keys are the shell's in a terminal.
	const action = actionFor(map, event, (each) => !isEditorAction(each) && each.group !== "Chat");
	if (!action) return false;
	const plainCtrl = event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
	return !(plainCtrl && /^Key[A-Z]$/.test(event.code)) || Boolean(action.overShell);
}
