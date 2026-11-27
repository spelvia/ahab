import { describe, expect, it } from "vitest";

import {
	detectLanguage,
	type LanguageDeclaration,
	languageNamed,
	modelineOf,
	nestSymbols,
	patternOf,
	resolveLanguage,
	shebangOf,
	spansOf,
	tokenClasses,
	tokensByLine,
	tooLarge,
} from "@/shared/syntax/model";

const declare = (declaration: Partial<LanguageDeclaration> & { id: string }, source = "bundled") =>
	resolveLanguage(
		{ name: declaration.id, grammar: { wasm: `${declaration.id}.wasm` }, ...declaration },
		"https://ahab.test/languages/",
		source,
	);

const languages = [
	declare({ id: "typescript", files: { extensions: ["ts"] }, aliases: ["ts"] }),
	declare({ id: "c", files: { extensions: ["c", "h"] } }),
	declare({
		id: "python",
		files: { extensions: ["py"], shebangs: ["python", "python3"] },
		aliases: ["py"],
	}),
	declare({ id: "dockerfile", files: { names: ["Dockerfile", "Dockerfile.*"] } }),
	declare({
		id: "bash",
		files: { extensions: ["sh"], names: [".bashrc"], shebangs: ["bash", "sh"] },
	}),
	declare({ id: "markdown.inline", hidden: true, files: { extensions: ["md"] } }),
];

describe("finding a file's language", () => {
	it("goes by extension, then name, then #!, then a modeline", () => {
		expect(detectLanguage(languages, "src/a.ts")?.id).toBe("typescript");
		expect(detectLanguage(languages, "Dockerfile.dev")?.id).toBe("dockerfile");
		expect(detectLanguage(languages, "home/.bashrc")?.id).toBe("bash");
		expect(detectLanguage(languages, "bin/tool", "#!/usr/bin/env -S python3 -u\nprint()")?.id).toBe(
			"python",
		);
		expect(detectLanguage(languages, "notes", "x\n# vim: set ft=sh:\n")?.id).toBe("bash");
		expect(detectLanguage(languages, "notes.txt", "plain")).toBeUndefined();
	});

	it("never picks a hidden language, and lets later languages win", () => {
		expect(detectLanguage(languages, "README.md")).toBeUndefined();
		const plugin = declare({ id: "cpp", files: { extensions: ["h"] } }, "cpp-plugin");
		expect(detectLanguage([...languages, plugin], "a.h")?.id).toBe("cpp");
	});

	it("reads #! lines and modelines", () => {
		expect(shebangOf("#!/bin/bash")).toBe("bash");
		expect(shebangOf("#!/usr/bin/env node")).toBe("node");
		expect(shebangOf("echo")).toBeUndefined();
		expect(modelineOf("// -*- mode: rust -*-")).toBe("rust");
	});

	it("knows languages by alias, any case", () => {
		expect(languageNamed(languages, "PY")?.id).toBe("python");
		expect(languageNamed(languages, "h")?.id).toBe("c");
		expect(languageNamed(languages, "nothing")).toBeUndefined();
	});

	it("makes paths absolute and keeps unknown fields for language tooling", () => {
		const language = resolveLanguage(
			{
				id: "x",
				name: "X",
				grammar: { wasm: "grammars/x.wasm" },
				queries: { highlights: "queries/x.scm" },
				servers: ["x-ls"],
			},
			"https://ahab.test/api/v1/plugins/p/files/",
			"p",
		);
		expect(language.wasm).toBe("https://ahab.test/api/v1/plugins/p/files/grammars/x.wasm");
		expect(language.queries.highlights).toBe(
			"https://ahab.test/api/v1/plugins/p/files/queries/x.scm",
		);
		expect(language.declaration.servers).toEqual(["x-ls"]);
	});

	it("names patterns for a choice per kind of file", () => {
		expect(patternOf("src/a.h")).toBe("*.h");
		expect(patternOf("Makefile")).toBe("Makefile");
	});
});

describe("tokens", () => {
	it("show as classes, from general to particular", () => {
		expect(tokenClasses("function.method")).toBe("tok-function tok-function-method");
	});

	it("split by line", () => {
		const text = "ab\ncd\nef";
		expect(tokensByLine(text, [{ from: 1, to: 7, tag: "string" }])).toEqual([
			[{ from: 1, to: 2, tag: "string" }],
			[{ from: 0, to: 2, tag: "string" }],
			[{ from: 0, to: 1, tag: "string" }],
		]);
	});

	it("cut a line into spans with marks", () => {
		expect(
			spansOf("let x = 1", [{ from: 0, to: 3, tag: "keyword" }], [{ from: 4, to: 5 }]),
		).toEqual([
			{ text: "let", tag: "keyword" },
			{ text: " " },
			{ text: "x", marked: true },
			{ text: " = 1" },
		]);
	});

	it("are not made for huge files or lines", () => {
		expect(tooLarge("short")).toBeNull();
		expect(tooLarge(`a\n${"x".repeat(10_001)}`)).toBe("line");
	});
});

it("nests symbols by what contains what", () => {
	const flat = [
		{ name: "m", kind: "module", from: 0, to: 100, nameFrom: 4, nameTo: 5 },
		{ name: "f", kind: "function", from: 10, to: 20, nameFrom: 13, nameTo: 14 },
		{ name: "g", kind: "function", from: 30, to: 40, nameFrom: 33, nameTo: 34 },
		{ name: "h", kind: "function", from: 120, to: 130, nameFrom: 123, nameTo: 124 },
	];
	const nested = nestSymbols(flat);
	expect(nested.map((symbol) => symbol.name)).toEqual(["m", "h"]);
	expect(nested[0]?.children.map((symbol) => symbol.name)).toEqual(["f", "g"]);
});
