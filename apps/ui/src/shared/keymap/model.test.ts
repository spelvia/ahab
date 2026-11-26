import { describe, expect, it } from "vitest";

import {
	actionFor,
	conflicts,
	editorActionsFor,
	forAhab,
	hintFor,
	keymapOf,
	keyOf,
	resolve,
	shown,
} from "@/shared/keymap/model";

function key(code: string, mods: Partial<KeyboardEvent> = {}): KeyboardEvent {
	return new KeyboardEvent("keydown", { code, ...mods });
}

describe("the keymap", () => {
	it("gives window keys the modifier browsers leave free, and theirs where it arrives", () => {
		for (const onMac of [true, false]) {
			const map = keymapOf({}, onMac);
			const free = onMac ? { ctrlKey: true } : { altKey: true };
			const browser = onMac ? { metaKey: true } : { ctrlKey: true };
			const id = (event: KeyboardEvent) => actionFor(map, event)?.id;
			expect(id(key("KeyT", free))).toBe("session.new");
			expect(id(key("KeyT", { ...free, shiftKey: true }))).toBe("window.reopen");
			expect(id(key("KeyW", free))).toBe("window.close");
			expect(id(key("BracketRight", free))).toBe("window.next");
			expect(id(key("BracketLeft", free))).toBe("window.previous");
			expect(id(key("Backslash", free))).toBe("window.split");
			expect(id(key("KeyT", browser))).toBe("session.new");
			expect(id(key("KeyW", browser))).toBe("window.close");
			expect(id(key("KeyK", browser))).toBe("palette");
			expect(id(key("KeyB", { ...browser, altKey: true }))).toBe("sidebar.right");
			expect(id(key("KeyF", { ...browser, shiftKey: true }))).toBe("search.find");
			expect(id(key("KeyT"))).toBeUndefined();
			expect(id(key("KeyX", free))).toBeUndefined();
			expect(id(key("Tab", { ctrlKey: true }))).toBe("window.next");
			expect(id(key("Tab", { ctrlKey: true, shiftKey: true }))).toBe("window.previous");
			expect(id(key("PageDown", { ctrlKey: true }))).toBe("window.next");
			expect(id(key("Backquote", free))).toBe("terminal.new");
			expect(id(key("Backquote", { ctrlKey: true }))).toBe("terminal.new");
		}
	});

	it("switches to the nth project with Alt (⌥) and a digit, on every platform", () => {
		for (const onMac of [false, true]) {
			const map = keymapOf({}, onMac);
			expect(actionFor(map, key("Digit1", { altKey: true }))?.id).toBe("project.1");
			expect(actionFor(map, key("Digit9", { altKey: true }))?.id).toBe("project.9");
			// The browser's own Ctrl/⌘+1 stays the browser's.
			expect(
				actionFor(map, key("Digit1", onMac ? { metaKey: true } : { ctrlKey: true })),
			).toBeUndefined();
		}
	});

	it("takes changed keys in place of Ahab's", () => {
		const map = keymapOf({ palette: ["Ctrl+Shift+KeyP"] }, false);
		expect(actionFor(map, key("KeyK", { ctrlKey: true }))).toBeUndefined();
		expect(actionFor(map, key("KeyP", { ctrlKey: true, shiftKey: true }))?.id).toBe("palette");
	});

	it("writes and shows keys the same way everywhere", () => {
		expect(resolve("Shift+Mod+KeyF", true)).toBe("Shift+Meta+KeyF");
		expect(resolve("Win+KeyT", false)).toBe("Alt+KeyT");
		expect(keyOf(key("KeyK", { metaKey: true, shiftKey: true }))).toBe("Shift+Meta+KeyK");
		expect(keyOf(key("ShiftLeft", { shiftKey: true }))).toBeUndefined();
		expect(shown("Ctrl+Shift+KeyT", true)).toBe("⌃⇧T");
		expect(shown("Alt+Backquote", false)).toBe("Alt+`");
	});

	it("hints the key a browser tab gets, or the app's own", () => {
		const map = keymapOf({}, false);
		expect(hintFor(map, "session.new", false, false)).toBe("Alt+T");
		expect(hintFor(map, "window.next", true, false)).toBe("Alt+]");
		expect(hintFor(map, "palette", false, false)).toBe("Ctrl+K");
	});

	it("leaves Ahab's shortcuts to Ahab in a terminal, the rest to the shell", () => {
		const map = keymapOf({}, false);
		expect(forAhab(map, key("KeyT", { altKey: true }), false)).toBe(true);
		expect(forAhab(map, key("Backquote", { altKey: true }), false)).toBe(true);
		expect(forAhab(map, key("KeyB", { altKey: true }), false)).toBe(false);
		expect(forAhab(map, key("KeyP", { ctrlKey: true }), false)).toBe(true);
		expect(forAhab(map, key("KeyF", { ctrlKey: true, shiftKey: true }), false)).toBe(true);
		expect(forAhab(map, key("KeyK", { ctrlKey: true }), false)).toBe(false);
		expect(forAhab(map, key("KeyW", { ctrlKey: true }), false)).toBe(false);
		const mine = keymapOf({ palette: ["Alt+KeyK"] }, false);
		expect(forAhab(mine, key("KeyK", { altKey: true }), false)).toBe(true);
		expect(forAhab(map, key("KeyK", { metaKey: true }), true)).toBe(true);
		expect(forAhab(map, key("KeyW", { ctrlKey: true }), true)).toBe(false);
	});

	it("gives the editor's commands their keys, or the ones changed in their place", () => {
		const map = keymapOf({}, false);
		expect(editorActionsFor(map, "Alt+ArrowUp")).toEqual(["editor.moveLineUp"]);
		expect(editorActionsFor(map, "Ctrl+Shift+KeyK")).toEqual(["editor.deleteLine"]);
		expect(editorActionsFor(map, "F12")).toEqual(["editor.goToDefinition"]);
		expect(editorActionsFor(map, "Ctrl+KeyD")).toEqual(["editor.selectNextOccurrence"]);
		expect(editorActionsFor(keymapOf({}, true), "Meta+KeyD")).toEqual([
			"editor.selectNextOccurrence",
		]);
		expect(editorActionsFor(map, "Ctrl+KeyK")).toEqual([]);
		const mine = keymapOf({ "editor.deleteLine": ["Ctrl+Shift+KeyD"] }, false);
		expect(editorActionsFor(mine, "Ctrl+Shift+KeyK")).toEqual([]);
		expect(editorActionsFor(mine, "Ctrl+Shift+KeyD")).toEqual(["editor.deleteLine"]);
		// Ahab's own keys and the editor's do not clash by default.
		expect(conflicts(keymapOf({}, false))).toEqual(new Map());
		expect(conflicts(keymapOf({}, true))).toEqual(new Map());
	});

	it("leaves the editor's keys to the shell in a terminal, and to the editor elsewhere", () => {
		const map = keymapOf({}, false);
		expect(forAhab(map, key("ArrowUp", { altKey: true }), false)).toBe(false);
		expect(forAhab(map, key("F12"), false)).toBe(false);
		expect(
			actionFor(map, key("ArrowUp", { altKey: true }), (action) => action.group !== "Editor"),
		).toBeUndefined();
	});
});

it("gives no key for an action the keymap does not know", () => {
	expect(hintFor(keymapOf({}), "project.0" as never, false)).toBeUndefined();
});
