import { describe, expect, it } from "vitest";

import { ctrlOf, findPaths, shownTitle } from "@/modules/sessions/terminal/model";
import type { Terminal } from "@/shared/api/generated/ahab.schemas";

const terminal = (change: Partial<Terminal>) =>
	({ title: null, command: null, ...change }) as Terminal;

function shown(title: string, change: Partial<Terminal>) {
	return shownTitle({ title, kind: "terminal", terminal: terminal(change) });
}

function paths(text: string) {
	return findPaths(text, "/work/app").map(({ path, line }) => [path, line]);
}

describe("terminals", () => {
	it("show the shell's title until renamed", () => {
		expect(shown("Terminal", { title: "vim main.rs" })).toBe("vim main.rs");
		expect(shown("Terminal", {})).toBe("Terminal");
		expect(shown("Build box", { title: "make" })).toBe("Build box");
		expect(shown("cargo test", { command: "cargo test", title: "cargo" })).toBe("cargo");
		expect(shownTitle({ title: "Chat", kind: "chat", terminal: null })).toBe("Chat");
	});

	it("find paths into the project in output", () => {
		expect(paths("error: src/main.rs:42:7 expected `;`")).toEqual([["src/main.rs", 42]]);
		expect(paths("  --> ./ui/app.tsx")).toEqual([["ui/app.tsx", undefined]]);
		expect(paths("at /work/app/lib/a.ts:3")).toEqual([["lib/a.ts", 3]]);
		expect(paths("at /elsewhere/a.ts:3 and main.rs:9")).toEqual([["main.rs", 9]]);
		expect(paths("see https://example.com/a.js or v1.2 or foo.bar")).toEqual([]);
	});

	it("make Ctrl combinations", () => {
		expect(ctrlOf("c")).toBe("\u0003");
		expect(ctrlOf("D")).toBe("\u0004");
		expect(ctrlOf("[")).toBe("\u001b");
		expect(ctrlOf("1")).toBe("1");
	});
});
