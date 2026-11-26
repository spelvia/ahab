import { describe, expect, it } from "vitest";

import {
	badName,
	diskChange,
	editorConfig,
	editorConfigMatches,
	editable,
	folderOf,
	joinPath,
	keepBothName,
	movedPath,
	planMoves,
	rangeOf,
	resolveIndentation,
	topLevelPaths,
	typeAhead,
	sortEntries,
} from "@/modules/projects/files/model";
import { resolveLink } from "@/shared/ui/markdown-links";

describe("resolveLink", () => {
	it("follows links between the project's files", () => {
		expect(resolveLink("rfcs/004-sessions.md", "002-daemon.md")).toEqual({
			path: "rfcs/002-daemon.md",
		});
		expect(resolveLink("rfcs/004-sessions.md", "003-projects.md#implementation-status")).toEqual({
			path: "rfcs/003-projects.md",
			anchor: "implementation-status",
		});
		expect(resolveLink("rfcs/a.md", "../docs/./b%20c.md")).toEqual({ path: "docs/b c.md" });
		expect(resolveLink("rfcs/a.md", "/README.md")).toEqual({ path: "README.md" });
	});

	it("keeps anchors in the same file, and refuses what leaves the project", () => {
		expect(resolveLink("rfcs/a.md", "#open-questions")).toEqual({ anchor: "open-questions" });
		expect(resolveLink("rfcs/a.md", "a.md#x")).toEqual({ anchor: "x" });
		expect(resolveLink("a.md", "../outside.md")).toBeUndefined();
		expect(resolveLink("a.md", "https://example.com")).toBeUndefined();
		expect(resolveLink("a.md", "mailto:me@example.com")).toBeUndefined();
	});
});

describe("editing", () => {
	it("answers a change on disk by what the window holds", () => {
		expect(diskChange("v1", false, "v1")).toBe("same");
		expect(diskChange("v1", false, "v2")).toBe("reload");
		expect(diskChange("v1", true, "v2")).toBe("conflict");
		expect(diskChange("v1", true, null)).toBe("deleted");
		expect(diskChange(null, true, null)).toBe("same");
	});

	it("edits only whole text files in writable projects", () => {
		const text = { binary: false, truncated: false, version: "v1" };
		expect(editable(text, false)).toBe(true);
		expect(editable(text, true)).toBe(false);
		expect(editable({ ...text, truncated: true }, false)).toBe(false);
		expect(editable({ ...text, binary: true }, false)).toBe(false);
		expect(editable({ ...text, version: null }, false)).toBe(false);
	});

	it("checks names and joins paths", () => {
		expect(badName("src/new.rs")).toBeUndefined();
		expect(badName(" ")).toBe("Enter a name.");
		expect(badName("../x")).toBeDefined();
		expect(badName("/etc/x")).toBeDefined();
		expect(joinPath("src", "a.rs")).toBe("src/a.rs");
		expect(joinPath("", "a.rs")).toBe("a.rs");
		expect(folderOf("src/a/b.rs")).toBe("src/a");
		expect(folderOf("b.rs")).toBe("");
	});
});

describe("editorConfig", () => {
	it("takes the nearest file's indentation and stops at a root", () => {
		const found = editorConfig([
			{ path: "main.go", text: "[*.go]\nindent_style = tab\ntab_width = 8\n" },
			{ path: "src/main.go", text: "root = true\n[*]\nindent_style = space\nindent_size = 2\n" },
			{ path: "app/src/main.go", text: "[*]\nindent_size = 6\n" },
		]);
		expect(found).toEqual({ useTabs: true, tabSize: 8 });
		expect(editorConfig([{ path: "a.ts", text: "[*.{js,ts}]\nindent_size = 2\n" }])).toEqual({
			tabSize: 2,
		});
		expect(editorConfigMatches("lib/**.py", "lib/a/b.py")).toBe(true);
		expect(editorConfigMatches("*.md", "docs/a.md")).toBe(true);
		expect(editorConfigMatches("*.md", "a.mdx")).toBe(false);
	});
});

describe("resolveIndentation", () => {
	const settings = { tabSize: 4, useTabs: false };
	it("takes the window's choice, then EditorConfig, then the text, then the settings", () => {
		expect(resolveIndentation({ settings })).toEqual({
			useTabs: false,
			tabSize: 4,
			source: "settings",
		});
		expect(resolveIndentation({ settings, detected: { useTabs: false, size: 2 } })).toEqual({
			useTabs: false,
			tabSize: 2,
			source: "detected",
		});
		expect(resolveIndentation({ settings, detected: { useTabs: true, size: 0 } })).toEqual({
			useTabs: true,
			tabSize: 4,
			source: "detected",
		});
		expect(
			resolveIndentation({
				settings,
				detected: { useTabs: false, size: 2 },
				editorconfig: { useTabs: true },
			}),
		).toEqual({ useTabs: true, tabSize: 2, source: "editorconfig" });
		expect(
			resolveIndentation({
				settings,
				editorconfig: { tabSize: 8 },
				chosen: { useTabs: false, tabSize: 3 },
			}),
		).toEqual({ useTabs: false, tabSize: 3, source: "window" });
		expect(resolveIndentation({ settings, editorconfig: {} }).source).toBe("settings");
	});
});

describe("moving by dragging", () => {
	it("moves each source into the folder, under its name", () => {
		expect(planMoves(["src/a.ts", "docs"], "lib")).toEqual({
			moves: [
				{ from: "src/a.ts", to: "lib/a.ts" },
				{ from: "docs", to: "lib/docs" },
			],
		});
		expect(planMoves(["src/a.ts"], "")).toEqual({ moves: [{ from: "src/a.ts", to: "a.ts" }] });
	});

	it("takes what is inside another source along, and leaves what is there already", () => {
		expect(planMoves(["src", "src/a.ts", "lib/b.ts", "src"], "lib").moves).toEqual([
			{ from: "src", to: "lib/src" },
		]);
		expect(planMoves(["a.ts"], "").moves).toEqual([]);
	});

	it("refuses a folder into itself or below it", () => {
		expect(planMoves(["src"], "src").refused).toBe("src cannot move into itself.");
		expect(planMoves(["a.ts", "src"], "src/deep").refused).toBe("src cannot move into itself.");
		expect(planMoves(["src"], "srcs").refused).toBeUndefined();
	});

	it("keeps both under a free name", () => {
		const taken = new Set(["a copy.ts"]);
		expect(keepBothName("a.ts", (name) => taken.has(name))).toBe("a copy 2.ts");
		expect(keepBothName("lib", () => false)).toBe("lib copy");
		expect(keepBothName(".env", () => false)).toBe(".env copy");
		expect(keepBothName("archive.tar.gz", () => false)).toBe("archive.tar copy.gz");
	});

	it("follows a moved file or folder", () => {
		expect(movedPath("src/a.ts", "src", "lib/src")).toBe("lib/src/a.ts");
		expect(movedPath("src", "src", "lib/src")).toBe("lib/src");
		expect(movedPath("srcs/a.ts", "src", "lib/src")).toBe("srcs/a.ts");
	});
});

describe("choosing in the tree", () => {
	it("acts on what is not inside another chosen folder", () => {
		expect(topLevelPaths(["src/a.ts", "src", "docs", "src", ""])).toEqual(["src", "docs"]);
	});

	it("takes a range either way round", () => {
		const order = ["a", "b", "c", "d"];
		expect(rangeOf(order, "b", "d")).toEqual(["b", "c", "d"]);
		expect(rangeOf(order, "c", "a")).toEqual(["a", "b", "c"]);
		expect(rangeOf(order, "gone", "c")).toEqual(["c"]);
		expect(rangeOf(order, "a", "gone")).toEqual([]);
	});

	it("jumps to the next name typed, wrapping, a letter again moving on", () => {
		const names = ["docs", "README.md", "rust", "src"];
		expect(typeAhead(names, "r", 0)).toBe(1);
		expect(typeAhead(names, "r", 1)).toBe(2);
		expect(typeAhead(names, "r", 2)).toBe(1);
		expect(typeAhead(names, "ru", 1)).toBe(2);
		expect(typeAhead(names, "re", 1)).toBe(1);
		expect(typeAhead(names, "rr", 1)).toBe(2);
		expect(typeAhead(names, "x", 0)).toBe(-1);
		expect(typeAhead(names, "d", -1)).toBe(0);
	});
});

describe("sortEntries", () => {
	const entries = [
		{ name: "b.ts", kind: "file", modifiedAt: "2026-11-02T00:00:00Z", size: 10 },
		{ name: "src", kind: "dir", modifiedAt: "2026-11-01T00:00:00Z", size: null },
		{ name: "a.md", kind: "file", modifiedAt: "2026-11-03T00:00:00Z", size: 30 },
		{ name: "A10.ts", kind: "file", modifiedAt: "2026-11-01T00:00:00Z", size: 20 },
		{ name: "a2.ts", kind: "file", modifiedAt: "2026-11-01T00:00:00Z", size: 20 },
	];
	const names = (by: "name" | "type" | "modified" | "size", foldersFirst = true) =>
		sortEntries(entries, { by, foldersFirst }).map((entry) => entry.name);

	it("orders by name (numbers as numbers), type, newest or largest, folders first", () => {
		expect(names("name")).toEqual(["src", "a.md", "a2.ts", "A10.ts", "b.ts"]);
		expect(names("type")).toEqual(["src", "a.md", "a2.ts", "A10.ts", "b.ts"]);
		expect(names("modified")).toEqual(["src", "a.md", "b.ts", "a2.ts", "A10.ts"]);
		expect(names("size")).toEqual(["src", "a.md", "a2.ts", "A10.ts", "b.ts"]);
		expect(names("modified", false)).toEqual(["a.md", "b.ts", "a2.ts", "A10.ts", "src"]);
	});
});
