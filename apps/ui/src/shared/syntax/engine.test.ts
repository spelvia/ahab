// @vitest-environment node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

import { describe, expect, it } from "vitest";

import type { HighlightToken } from "@/shared/editor/model";
import { gzipped, mergeLines, paint, SyntaxDocument, SyntaxEngine } from "@/shared/syntax/engine";
import { type LanguageDeclaration, resolveLanguage } from "@/shared/syntax/model";

const folder = new URL("../../../languages/", import.meta.url);
const declarations = JSON.parse(
	readFileSync(new URL("languages.json", folder), "utf8"),
) as LanguageDeclaration[];
const languages = declarations.map((declaration) =>
	resolveLanguage(declaration, folder.href, "bundled"),
);

const engine = new SyntaxEngine({
	bytes: async (url) => {
		const bytes = new Uint8Array(readFileSync(fileURLToPath(url)));
		return gzipped(bytes) ? new Uint8Array(gunzipSync(bytes)) : bytes;
	},
	text: async (url) => readFileSync(fileURLToPath(url), "utf8"),
});
engine.setLanguages(languages);

async function loaded(id: string) {
	const language = engine.language(id);
	if (!language) throw new Error(`no ${id}`);
	const found = await engine.load(language);
	if (!found) throw new Error(`${id}: ${engine.problems.get(id)}`);
	return found;
}

/** The tag on the first occurrence of `piece` in `text`. */
function tagOf(tokens: HighlightToken[], text: string, piece: string, nth = 0): string | undefined {
	let at = -1;
	for (let i = 0; i <= nth; i++) at = text.indexOf(piece, at + 1);
	return tokens.find((token) => token.from <= at && token.to >= at + piece.length)?.tag;
}

/** One small sample per bundled language, and pieces of it with the capture they should get. */
const SAMPLES: Record<string, { text: string; expect: [string, RegExp][] }> = {
	bash: {
		text: 'if [ -n "$x" ]; then echo "hi"; fi\n',
		expect: [
			["if", /^keyword/],
			['"hi"', /^string/],
		],
	},
	c: {
		text: "int main(void) { return 0; } // done\n",
		expect: [
			["return", /^keyword/],
			["// done", /^comment/],
		],
	},
	"c-sharp": {
		text: 'class A { string s = "x"; }\n',
		expect: [
			["class", /^keyword/],
			['"x"', /^string/],
		],
	},
	cpp: {
		text: "namespace n { auto f() -> int { return 1; } }\n",
		expect: [
			["namespace", /^keyword/],
			["1", /^constant/],
		],
	},
	css: { text: "a { color: red; }\n", expect: [["color", /^variable|^property|^attribute/]] },
	dockerfile: { text: "FROM debian:12\nRUN echo hi\n", expect: [["FROM", /^keyword/]] },
	go: {
		text: 'package main\nfunc main() { println("hi") }\n',
		expect: [
			["func", /^keyword/],
			['"hi"', /^string/],
		],
	},
	html: {
		text: '<p class="a">Hi</p>\n',
		expect: [
			["p", /^tag/],
			["class", /^attribute/],
		],
	},
	java: {
		text: 'class A { void f() { String s = "x"; } }\n',
		expect: [
			["class", /^keyword/],
			['"x"', /^string/],
		],
	},
	javascript: {
		text: 'const a = () => <div className="x">{1}</div>;\n',
		expect: [
			["const", /^keyword/],
			["div", /^tag/],
		],
	},
	json: {
		text: '{"a": [1, true, null]}\n',
		expect: [
			['"a"', /^(variable|string|property)/],
			["1", /^constant/],
		],
	},
	kotlin: {
		text: 'fun main() { val s = "x" }\n',
		expect: [
			["fun", /^keyword/],
			['"x"', /^string/],
		],
	},
	lua: {
		text: 'local function f() return "x" end\n',
		expect: [
			["local", /^keyword/],
			['"x"', /^string/],
		],
	},
	make: { text: "all: main.o\n\tcc -o all main.o\n", expect: [["cc", /^function/]] },
	markdown: { text: "# Title\n\nSome *text*.\n", expect: [["Title", /^markup/]] },
	nix: {
		text: '{ pkgs }: let a = "x"; in a\n',
		expect: [
			["let", /^keyword/],
			['"x"', /^string/],
		],
	},
	php: {
		text: '<?php function f() { return "x"; }\n',
		expect: [
			["function", /^keyword/],
			['"x"', /^string/],
		],
	},
	python: {
		text: 'def greet(x):\n    return "hi"\n',
		expect: [
			["def", /^keyword/],
			["greet", /^function/],
			['"hi"', /^string/],
		],
	},
	ruby: {
		text: 'def f\n  "x"\nend\n',
		expect: [
			["def", /^keyword/],
			['"x"', /^string/],
		],
	},
	rust: {
		text: 'fn main() { let s = "hi"; }\n',
		expect: [
			["fn", /^keyword/],
			["main", /^function/],
			['"hi"', /^string/],
		],
	},
	scss: { text: "$a: 1px;\n.b { color: $a; }\n", expect: [["1", /^constant/]] },
	sql: {
		text: "SELECT a FROM t WHERE b = 'x';\n",
		expect: [
			["SELECT", /^keyword/],
			["'x'", /^string/],
		],
	},
	swift: {
		text: 'func f() -> String { return "x" }\n',
		expect: [
			["func", /^keyword/],
			['"x"', /^string/],
		],
	},
	toml: { text: '[a]\nb = "x"\n', expect: [['"x"', /^string/]] },
	tsx: {
		text: "const a: number = <b>1</b>;\n",
		expect: [
			["const", /^keyword/],
			["number", /^type/],
		],
	},
	typescript: {
		text: "interface A { b: string }\n",
		expect: [
			["interface", /^keyword/],
			["string", /^type/],
		],
	},
	yaml: {
		text: "a: 'x'\nb: [1, 2]\n",
		expect: [
			["'x'", /^string/],
			["1", /^constant/],
		],
	},
};

describe("the bundled languages", () => {
	it("have a sample each", () => {
		const choosable = declarations.filter((item) => !item.hidden).map((item) => item.id);
		expect(Object.keys(SAMPLES).toSorted()).toEqual(choosable.toSorted());
	});

	for (const [id, sample] of Object.entries(SAMPLES)) {
		it(`highlights ${id}`, async () => {
			const language = await loaded(id);
			// Every query compiled.
			expect(engine.problems.get(id)).toBeUndefined();
			const tokens = await engine.highlightText(language, sample.text);
			expect(tokens.length).toBeGreaterThan(0);
			for (const [piece, tag] of sample.expect)
				expect(tagOf(tokens, sample.text, piece)).toMatch(tag);
			// Tokens do not overlap and stay inside the text.
			for (let i = 1; i < tokens.length; i++)
				expect(tokens[i]?.from).toBeGreaterThanOrEqual(tokens[i - 1]?.to ?? 0);
			expect(tokens.at(-1)?.to).toBeLessThanOrEqual(sample.text.length);
		});
	}
});

describe("injections", () => {
	it("highlights code in Markdown fences with its own language", async () => {
		const text = "# Notes\n\n```rust\nfn main() {}\n```\n\nSome `code` and **bold**.\n";
		const tokens = await engine.highlightText(await loaded("markdown"), text);
		expect(tagOf(tokens, text, "fn")).toMatch(/^keyword/);
		expect(tagOf(tokens, text, "main")).toMatch(/^function/);
		// Inline Markdown, injected too.
		expect(tagOf(tokens, text, "bold")).toMatch(/^markup\.bold/);
	});

	it("knows a fence's language by an alias", async () => {
		const text = "```py\ndef f(): pass\n```\n";
		const tokens = await engine.highlightText(await loaded("markdown"), text);
		expect(tagOf(tokens, text, "def")).toMatch(/^keyword/);
	});

	it("highlights JavaScript and CSS inside HTML", async () => {
		const text = "<style>a { color: red; }</style>\n<script>const x = 1;</script>\n";
		const tokens = await engine.highlightText(await loaded("html"), text);
		expect(tagOf(tokens, text, "const")).toMatch(/^keyword/);
		expect(tagOf(tokens, text, "1")).toMatch(/^constant/);
		expect(tagOf(tokens, text, "color")).toBeDefined();
	});

	it("leaves a fence in an unknown language plain", async () => {
		const text = "```nosuchlanguage\nfn main() {}\n```\n";
		const tokens = await engine.highlightText(await loaded("markdown"), text);
		expect(tagOf(tokens, text, "fn")).not.toMatch(/^keyword/);
	});
});

describe("a document", () => {
	it("parses again after edits and says what to highlight again", async () => {
		const language = await loaded("rust");
		const document = new SyntaxDocument(engine, language, "fn a() {}\nfn b() {}\n", 0);
		// Insert a line between the two functions, and rename b.
		const ranges = document.update(
			[
				{ from: 10, to: 10, insert: 'fn c() { "x" }\n' },
				{ from: 13, to: 14, insert: "bee" },
			],
			1,
		);
		expect(document.text).toBe('fn a() {}\nfn c() { "x" }\nfn bee() {}\n');
		expect(document.version).toBe(1);
		const tree = document.current;
		expect(tree?.rootNode.hasError).toBe(false);
		expect(ranges.length).toBeGreaterThan(0);
		for (const range of ranges) expect(range.to).toBeLessThanOrEqual(document.text.length);
		if (!tree) return;
		const tokens = await engine.highlight(language, tree, document.text);
		expect(tagOf(tokens, document.text, "bee")).toMatch(/^function/);
		expect(tagOf(tokens, document.text, '"x"')).toMatch(/^string/);
		document.dispose();
	});

	it("opens a block comment and highlights to the end", async () => {
		const language = await loaded("rust");
		const document = new SyntaxDocument(engine, language, "fn a() {}\nfn b() {}\n", 0);
		const ranges = document.update([{ from: 0, to: 0, insert: "/* " }], 1);
		// To the end of the last line.
		expect(ranges.at(-1)?.to).toBe(document.text.length - 1);
		document.dispose();
	});
});

describe("structure", () => {
	const rust =
		"mod m {\n    pub struct S {\n        a: u8,\n    }\n\n    fn f() {\n        let x = 1;\n    }\n}\n";

	it("folds blocks, keeping the closing line", async () => {
		const language = await loaded("rust");
		const tree = engine.parse(language, rust);
		if (!tree) throw new Error("no tree");
		const folds = engine.folds(language, tree, rust);
		const lines = folds.map((fold) => [
			rust.slice(0, fold.from).split("\n").length,
			rust.slice(0, fold.to).split("\n").length,
		]);
		// mod m (lines 1–8), struct S (2–3), fn f (6–7): each closing brace stays.
		expect(lines).toEqual([
			[1, 8],
			[2, 3],
			[6, 7],
		]);
		tree.delete();
	});

	it("outlines definitions, nested", async () => {
		const language = await loaded("rust");
		const tree = engine.parse(language, rust);
		if (!tree) throw new Error("no tree");
		const symbols = engine.symbols(language, tree);
		expect(symbols.map((symbol) => symbol.name)).toEqual(["m"]);
		expect(symbols[0]?.children.map((symbol) => [symbol.name, symbol.kind])).toEqual([
			["S", "struct"],
			["f", "function"],
		]);
		tree.delete();
	});

	it("outlines TypeScript's functions and classes too", async () => {
		const language = await loaded("typescript");
		const text = "export function f() {}\nclass C {\n  m(): void {}\n}\ninterface I {}\n";
		const tree = engine.parse(language, text);
		if (!tree) throw new Error("no tree");
		const symbols = engine.symbols(language, tree);
		expect(symbols.map((symbol) => [symbol.name, symbol.kind])).toEqual([
			["f", "function"],
			["C", "class"],
			["I", "interface"],
		]);
		expect(symbols[1]?.children.map((symbol) => symbol.name)).toEqual(["m"]);
		tree.delete();
	});

	it("outlines Markdown by its headings' sections", async () => {
		const language = await loaded("markdown");
		const text = "# A\n\ntext\n\n## B\n\nmore\n\n# C\n";
		const tree = engine.parse(language, text);
		if (!tree) throw new Error("no tree");
		const symbols = engine.symbols(language, tree);
		expect(symbols.map((symbol) => symbol.name)).toEqual(["A", "C"]);
		expect(symbols[0]?.children.map((symbol) => symbol.name)).toEqual(["B"]);
		tree.delete();
	});

	it("finds definitions and references by name", async () => {
		const language = await loaded("python");
		const text = "def greet(name):\n    return name\n\ngreet('x')\n";
		const tree = engine.parse(language, text);
		if (!tree) throw new Error("no tree");
		const tags = engine.tags(language, tree);
		expect(tags.find((tag) => tag.role === "definition")).toMatchObject({
			name: "greet",
			kind: "function",
			line: 1,
		});
		expect(
			tags.some((tag) => tag.role === "reference" && tag.name === "greet" && tag.line === 4),
		).toBe(true);
		expect(engine.nameAt(tree, text.lastIndexOf("greet") + 2)).toMatchObject({ name: "greet" });
		tree.delete();
	});

	it("selects the syntax around a selection, larger each time", async () => {
		const language = await loaded("rust");
		const tree = engine.parse(language, rust);
		if (!tree) throw new Error("no tree");
		const at = rust.indexOf("1;");
		const ranges = engine.selectionRanges(tree, at, at);
		expect(rust.slice(ranges[0]?.from, ranges[0]?.to)).toBe("1");
		expect(ranges.map((range) => rust.slice(range.from, range.to))).toContain("let x = 1;");
		for (let i = 1; i < ranges.length; i++)
			expect((ranges[i]?.to ?? 0) - (ranges[i]?.from ?? 0)).toBeGreaterThan(
				(ranges[i - 1]?.to ?? 0) - (ranges[i - 1]?.from ?? 0),
			);
		tree.delete();
	});
});

describe("painting", () => {
	it("lets inner nodes and later patterns win", () => {
		expect(
			paint(
				[
					{ from: 0, to: 10, tag: "string", layer: 0, pattern: 5 },
					{ from: 2, to: 4, tag: "variable", layer: 0, pattern: 1 },
					{ from: 2, to: 4, tag: "function", layer: 0, pattern: 3 },
				],
				0,
				10,
			),
		).toEqual([
			{ from: 0, to: 2, tag: "string" },
			{ from: 2, to: 4, tag: "function" },
			{ from: 4, to: 10, tag: "string" },
		]);
	});

	it("widens ranges to whole lines", () => {
		expect(mergeLines("ab\ncd\nef", [{ from: 4, to: 4 }])).toEqual([{ from: 3, to: 5 }]);
		expect(
			mergeLines("ab\ncd\nef", [
				{ from: 1, to: 1 },
				{ from: 4, to: 7 },
			]),
		).toEqual([{ from: 0, to: 8 }]);
	});
});
