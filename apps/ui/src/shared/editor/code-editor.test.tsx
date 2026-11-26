import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { CodeEditor, type EditorHandle } from "@/shared/editor/code-editor";
import { DEFAULT_SETTINGS, type LanguageService, type Location } from "@/shared/editor/model";
import { registerHighlighter, registerLanguageService } from "@/shared/editor/registry";
import { VimStatusItem } from "@/shared/editor/vim-status-item";
import { resetShortcuts, setShortcut } from "@/shared/keymap/store";
import { setVimMappings } from "@/shared/keymap/vim";

beforeAll(() => {
	// jsdom lays nothing out; CodeMirror measures text with these.
	Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
	Range.prototype.getBoundingClientRect = () => new DOMRect();
});

const found: Location[] = [{ path: "src/b.ts", line: 3 }];
let stopService: (() => void) | undefined;

afterEach(() => {
	stopService?.();
	stopService = undefined;
	resetShortcuts();
	setVimMappings([]);
});

function serve(service: LanguageService) {
	stopService = registerLanguageService(() => service);
}

function open(vim: boolean, props: Partial<React.ComponentProps<typeof CodeEditor>> = {}) {
	const ref = createRef<EditorHandle>();
	const onLocations = vi.fn<(locations: Location[], kind: string) => void>();
	const onSave = vi.fn<() => void>();
	const view = render(
		<>
			<CodeEditor
				ref={ref}
				projectId="w"
				path="a.ts"
				initial={"one\ntwo\nthree"}
				readOnly={false}
				settings={{ ...DEFAULT_SETTINGS, vim }}
				onLocations={onLocations}
				onSave={onSave}
				{...props}
			/>
			<VimStatusItem />
		</>,
	);
	const content = view.container.querySelector<HTMLElement>(".cm-content")!;
	return { ref, content, onLocations, onSave, container: view.container };
}

function press(target: HTMLElement, key: string, init: KeyboardEventInit = {}) {
	act(() => {
		fireEvent.keyDown(target, { key, ...init });
	});
}

function type(target: HTMLElement, keys: string) {
	for (const key of keys)
		press(target, key, { code: /[a-z]/.test(key) ? `Key${key.toUpperCase()}` : "" });
}

async function vimReady(container: HTMLElement, content: HTMLElement) {
	await waitFor(() => expect(container.querySelector(".cm-vimMode")).not.toBeNull());
	act(() => content.focus());
}

describe("CodeEditor's keys", () => {
	it("runs the editor's commands by the keys Settings → Keyboard gives them", () => {
		const { ref, content } = open(false);
		act(() => ref.current?.reveal(5, 5));
		press(content, "ArrowUp", { code: "ArrowUp", altKey: true });
		expect(ref.current?.text()).toBe("two\none\nthree");

		act(() => setShortcut("editor.moveLineUp", ["Ctrl+Shift+KeyU"]));
		act(() => ref.current?.reveal(5, 5));
		press(content, "ArrowUp", { code: "ArrowUp", altKey: true });
		expect(ref.current?.text()).toBe("two\none\nthree");
		press(content, "U", { code: "KeyU", ctrlKey: true, shiftKey: true });
		expect(ref.current?.text()).toBe("one\ntwo\nthree");
	});

	it("goes to the definition with F12 and its other keys, when the tools find one", async () => {
		serve({ definition: async () => found });
		const { content, onLocations } = open(false);
		press(content, "F12", { code: "F12" });
		await waitFor(() => expect(onLocations).toHaveBeenCalledWith(found, "definition"));
	});
});

describe("CodeEditor's Vim", () => {
	it("shows Vim's mode and the keys of a pending command in the status bar", async () => {
		const { content, container } = open(true);
		await vimReady(container, content);
		expect(screen.getByRole("status")).toHaveTextContent("-- NORMAL --");
		type(content, "d");
		expect(screen.getByTitle("Vim mode: NORMAL, keys typed: d")).toBeInTheDocument();
		type(content, "2");
		expect(screen.getByTitle("Vim mode: NORMAL, keys typed: d2")).toBeInTheDocument();
		press(content, "Escape", { code: "Escape" });
		type(content, "i");
		expect(screen.getByRole("status")).toHaveTextContent("-- INSERT --");
		press(content, "Escape", { code: "Escape" });
		type(content, "V");
		expect(screen.getByRole("status")).toHaveTextContent("-- VISUAL LINE --");
	});

	it("goes to the definition with gd, and keeps Vim's gi where nothing finds implementations", async () => {
		serve({ definition: async () => found, references: async () => found });
		const { content, container, onLocations } = open(true);
		await vimReady(container, content);
		type(content, "gd");
		await waitFor(() => expect(onLocations).toHaveBeenCalledWith(found, "definition"));
		type(content, "gr");
		await waitFor(() => expect(onLocations).toHaveBeenCalledWith(found, "references"));
		type(content, "gi");
		expect(screen.getByRole("status")).toHaveTextContent("-- INSERT --");
	});

	it("comments lines out with gcc, and shows the hover with K until Escape", async () => {
		const stopHighlighter = registerHighlighter({
			open: () => ({ update() {}, dispose() {}, comments: () => ({ line: "//" }) }),
		});
		serve({ hover: async () => ({ from: 0, to: 3, markdown: "`one`: a word" }) });
		const { ref, content, container } = open(true);
		await vimReady(container, content);
		type(content, "gcc");
		expect(ref.current?.text()).toBe("// one\ntwo\nthree");
		type(content, "u");
		type(content, "K");
		await waitFor(() => expect(container.querySelector(".cm-tooltip-ahab-hover")).not.toBeNull());
		press(content, "Escape", { code: "Escape" });
		expect(container.querySelector(".cm-tooltip-ahab-hover")).toBeNull();
		stopHighlighter();
	});

	it("applies this device's mappings", async () => {
		const { content, container } = open(true);
		await vimReady(container, content);
		act(() => setVimMappings([{ mode: "insert", lhs: "jk", rhs: "<Esc>" }]));
		type(content, "i");
		expect(screen.getByRole("status")).toHaveTextContent("-- INSERT --");
		type(content, "jk");
		expect(screen.getByRole("status")).toHaveTextContent("-- NORMAL --");
	});
});
