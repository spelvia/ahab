import { describe, expect, it } from "vitest";

import {
	applyEdits,
	hoverMarkdown,
	pluginFixes,
	positionsOf,
	replaceEdits,
	statusOf,
	storedDiagnostic,
	toDiagnostics,
	toFileEdits,
	toLocations,
} from "@/modules/tools/lsp/model";

const range = (line: number, from: number, to: number) => ({
	start: { line, character: from },
	end: { line, character: to },
});

const tool = (state: string, kind = "languageServer") => ({ id: state, name: state, kind, state });

describe("LSP in the editor's terms", () => {
	it("maps diagnostics, hovers and places", () => {
		const doc = positionsOf("fn main() {}\nlet x = 1;\n");
		const [diagnostic, tagged] = toDiagnostics(
			[
				{ range: range(1, 4, 5), severity: 2, message: "unused", code: "W1" },
				{
					range: range(0, 3, 7),
					severity: 4,
					message: "old",
					code: 5,
					source: "ts",
					codeDescription: { href: "https://example.com/5" },
					tags: [1, 2, 9],
				},
			],
			doc,
			"fake",
		);
		expect(diagnostic).toEqual({
			from: 17,
			to: 18,
			severity: "warning",
			message: "unused",
			source: "fake",
			code: "W1",
		});
		expect(tagged).toMatchObject({
			severity: "hint",
			source: "ts",
			code: "5",
			codeHref: "https://example.com/5",
			tags: ["unnecessary", "deprecated"],
		});
		expect(
			storedDiagnostic({
				line: 2,
				column: 18,
				endLine: 2,
				endColumn: 24,
				severity: "warning",
				code: "E1",
				source: "clippy",
				message: "m",
			}),
		).toEqual({
			range: range(1, 17, 23),
			severity: 2,
			code: "E1",
			source: "clippy",
			message: "m",
		});
		expect(hoverMarkdown([{ language: "rust", value: "fn main()" }, "Docs"])).toBe(
			"```rust\nfn main()\n```\n\nDocs",
		);
		const places = toLocations(
			[
				{
					targetUri: "external:t1",
					targetRange: range(9, 0, 3),
					targetSelectionRange: range(10, 4, 7),
				},
			],
			{ t1: { label: "rust core", name: "option.rs", display: "/x/option.rs" } },
		);
		expect(places).toEqual([
			{ path: "external:t1", line: 11, character: 4, origin: "rust core", name: "option.rs" },
		]);
	});

	it("applies edits to files not open", () => {
		expect(
			applyEdits("a b c\nd", [
				{ range: range(0, 2, 3), newText: "B" },
				{ range: range(1, 0, 1), newText: "D" },
			]),
		).toBe("a B c\nD");
		expect(replaceEdits("let  x = 1;", "let x = 1;")).toEqual([{ from: 4, to: 5, insert: "" }]);
		expect(
			toFileEdits({ changes: { "a.rs": [{ range: range(0, 0, 1), newText: "b" }] } }),
		).toHaveLength(1);
	});

	it("says what serves a file", () => {
		const base = { path: "src/main.rs", language: "rust" };
		expect(statusOf({ ...base, tools: [tool("running")] }, "Rust").label).toBe("Rust ✓");
		expect(statusOf({ ...base, tools: [tool("starting")] }, "Rust").label).toBe("Rust · starting…");
		expect(
			statusOf({ ...base, tools: [tool("running"), tool("missing", "formatter")] }, "Rust").label,
		).toBe("Rust · 1 issue");
		expect(statusOf({ ...base, tools: [tool("missing")] }, "Java").label).toBe("Java · basic");
		expect(statusOf({ ...base, tools: [] }, "Markdown").label).toBe("Markdown");
		const held = statusOf(
			{ ...base, tools: [{ ...tool("held"), reason: "Pin this repository to start them." }] },
			"Java",
		);
		expect(held.label).toBe("Java · not started");
		expect(held.notes).toContain("Pin this repository to start them.");
	});
});

const problem = (line: number, fixes: unknown) => ({
	range: range(line, 2, 7),
	message: "A note",
	data: { fixes },
});

describe("plugins' fixes", () => {
	const fix = {
		title: "Call it “Heading”",
		edits: [
			{
				range: { start: { line: 0, character: 2 }, end: { line: 0, character: 7 } },
				newText: "Heading",
			},
		],
	};

	it("offers the fixes of the problems on the lines asked, as LSP edits", () => {
		const found = pluginFixes([["notes.notes", [problem(0, [fix]), problem(4, [fix])]]], 0, 0);
		expect(found).toEqual([
			{ title: "Call it “Heading”", source: "notes.notes", edits: fix.edits },
		]);
		expect(pluginFixes([["notes.notes", [problem(4, [fix])]]], 0, 2)).toEqual([]);
	});

	it("fills in what a plugin may leave out and drops what is not a fix", () => {
		const short = { title: "Insert", edits: [{ range: { start: { line: 0 } }, newText: "x" }] };
		const found = pluginFixes(
			[
				[
					"notes.notes",
					[
						problem(0, [
							short,
							{ title: "", edits: fix.edits },
							{ title: "No edits", edits: [] },
							{ title: "Bad edit", edits: [{ range: { start: { line: -1 } }, newText: "x" }] },
							"nonsense",
						]),
						problem(0, "not a list"),
						{ range: range(0, 0, 1), message: "a server's", data: { kind: 3 } },
					],
				],
			],
			0,
			0,
		);
		expect(found).toEqual([
			{
				title: "Insert",
				source: "notes.notes",
				edits: [
					{
						range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
						newText: "x",
					},
				],
			},
		]);
	});

	it("keeps a stored problem's fixes", () => {
		const stored = storedDiagnostic({
			line: 1,
			column: 3,
			endLine: 1,
			endColumn: 8,
			severity: "warning",
			source: "notes.notes",
			message: "A note",
			fixes: [fix],
		});
		expect(pluginFixes([["notes.notes", [stored]]], 0, 0)[0]?.edits).toEqual(fix.edits);
	});

	it("applies to the text as the fix says", () => {
		expect(applyEdits("# Title\n", fix.edits)).toBe("# Heading\n");
	});
});
