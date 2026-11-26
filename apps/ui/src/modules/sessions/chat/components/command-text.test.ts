import { describe, expect, it } from "vitest";

import {
	diffPath,
	outputKind,
	readWord,
	shellSummary,
	unwrapShell,
} from "@/modules/sessions/chat/components/command-text";

describe("unwrapShell", () => {
	it("removes the wrapper and unescapes single quotes", () => {
		expect(unwrapShell(`/bin/zsh -lc 'echo '\\''hi'\\'' && ls'`)).toEqual({
			script: "echo 'hi' && ls",
			wrapper: "/bin/zsh -lc",
		});
	});

	it("unescapes double-quoted scripts and keeps their lines", () => {
		expect(unwrapShell(`bash -c "printf \\"%s\\" \\$HOME\nls"`).script).toBe(
			'printf "%s" $HOME\nls',
		);
	});

	it("reads separate flags and ANSI-C quoting", () => {
		expect(unwrapShell(`bash -l -c $'a\\nb'`)).toEqual({ script: "a\nb", wrapper: "bash -l -c" });
	});

	it("leaves commands without a whole wrapper alone", () => {
		expect(unwrapShell("ls -la").script).toBe("ls -la");
		expect(unwrapShell(`bash -c 'ls' && rm x`).script).toBe(`bash -c 'ls' && rm x`);
		expect(unwrapShell(`bash -c 'open`).script).toBe(`bash -c 'open`);
		expect(unwrapShell("bash script.sh").wrapper).toBeUndefined();
	});
});

describe("shellSummary", () => {
	it("shows the first line that does something", () => {
		expect(shellSummary(`zsh -lc 'set -euo pipefail\n# write\ncat > a.py <<EOF\nx\nEOF'`)).toBe(
			"cat > a.py <<EOF …",
		);
		expect(shellSummary(`zsh -lc 'git status'`)).toBe("git status");
	});
});

describe("readWord", () => {
	it("joins quoted pieces and stops at operators", () => {
		expect(readWord(`'a'"b"c;d`, 0)).toEqual({ value: "abc", end: 7 });
	});
});

describe("outputKind", () => {
	it("recognizes JSON and unified diffs only", () => {
		expect(outputKind('{"a": [1, 2]}\n')).toBe("json");
		expect(outputKind("{not json")).toBe("terminal");
		expect(outputKind("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b")).toBe("diff");
		expect(outputKind("--- notes\nplain")).toBe("terminal");
		expect(outputKind('\u001b[32m{"a":1}\u001b[0m')).toBe("terminal");
		expect(outputKind(`[${"1,".repeat(200_000)}1]`)).toBe("terminal");
	});

	it("finds the one file a diff changes", () => {
		expect(diffPath("--- a/x.rs\n+++ b/x.rs\n")).toBe("x.rs");
		expect(diffPath("+++ b/a\n+++ b/b\n")).toBeUndefined();
	});
});
