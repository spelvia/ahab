import { describe, expect, it } from "vitest";

import { splitReferences } from "@/modules/sessions/reference/model";

describe("splitReferences", () => {
	it("reads back what the server sends", () => {
		const sent = [
			"Why is this slow?",
			"",
			"Context: src/join.rs, lines 40–58 (changed on disk since it was selected)",
			"````rust",
			"let a = 1;",
			"```nested```",
			"````",
			"",
			"Context: README.md (the whole file)",
			"",
			"Context: the diff of src/a.rs, line 3",
			"```diff",
			"+ added",
			"```",
			"",
			"Context: terminal output from cargo test",
			"```text",
			"1 failed",
			"```",
			"",
			"Context: search results for join",
			"```text",
			"src/a.rs:1: join",
			"```",
		].join("\n");
		const { text, references } = splitReferences(sent);
		expect(text).toBe("Why is this slow?");
		expect(references).toEqual([
			{
				kind: "lines",
				path: "src/join.rs",
				startLine: 40,
				endLine: 58,
				stale: true,
				text: "let a = 1;\n```nested```",
			},
			{ kind: "file", path: "README.md", stale: false, text: undefined },
			{
				kind: "diffHunk",
				path: "src/a.rs",
				startLine: 3,
				endLine: 3,
				stale: false,
				text: "+ added",
			},
			{ kind: "terminalOutput", label: "cargo test", stale: false, text: "1 failed" },
			{ kind: "searchResults", label: "join", stale: false, text: "src/a.rs:1: join" },
		]);
	});

	it("reads back a fork's chat history", () => {
		const { text, references } = splitReferences(
			"Go on\n\nContext: chat history from Fix the join\n```markdown\n## You\n\nHi\n```",
		);
		expect(text).toBe("Go on");
		expect(references).toEqual([
			{ kind: "chatHistory", stale: false, label: "Fix the join", text: "## You\n\nHi" },
		]);
	});

	it("reads a message of context only", () => {
		const { text, references } = splitReferences("Context: a.txt, line 3\n```txt\nx\n```");
		expect(text).toBe("");
		expect(references).toHaveLength(1);
	});

	it("leaves a message that only looks like it alone", () => {
		for (const message of [
			"Context: see below\nand more words",
			"Hello\n\nContext: a.rs, line 1\n```rs\nnever closed",
			"Plain text",
		]) {
			expect(splitReferences(message)).toEqual({ text: message, references: [] });
		}
	});
});
