import { Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import {
	cursorLabel,
	detectIndentation,
	detectLineEnding,
	encodingOf,
	indentFold,
	indentationLabel,
	lineSelections,
	linesLabel,
	nextProblemIndex,
	problemRange,
	separatorOf,
	visualColumn,
	worstSeverity,
} from "@/shared/editor/model";

const doc = Text.of(["one", "two", "three", "four", "five"]);
const at = (line: number, column = 0) => doc.line(line).from + column;

describe("lineSelections", () => {
	it("takes partial lines as whole lines", () => {
		expect(lineSelections(doc, [{ from: at(2, 1), to: at(3, 2) }])).toEqual([
			{ startLine: 2, endLine: 3, text: "two\nthree" },
		]);
	});

	it("leaves out the line a selection ends at the start of, and cursors", () => {
		expect(lineSelections(doc, [{ from: at(1), to: at(3) }])).toEqual([
			{ startLine: 1, endLine: 2, text: "one\ntwo" },
		]);
		expect(lineSelections(doc, [{ from: at(4), to: at(4) }])).toEqual([]);
	});

	it("keeps separate selections apart and merges those that touch", () => {
		const found = lineSelections(doc, [
			{ from: at(5, 1), to: at(5, 3) },
			{ from: at(1), to: at(1, 2) },
			{ from: at(2), to: at(2, 1) },
		]);
		expect(found.map((item) => [item.startLine, item.endLine])).toEqual([
			[1, 2],
			[5, 5],
		]);
	});
});

describe("indentFold", () => {
	const lines = ["fn a() {", "    one", "", "    two", "}", "next"];
	const text = (n: number) => lines[n - 1] ?? "";
	it("folds the deeper lines after a line, blank ones inside", () => {
		expect(indentFold(text, lines.length, 1, 4)).toEqual({ endLine: 4 });
		expect(indentFold(text, lines.length, 2, 4)).toBeNull();
		expect(indentFold(text, lines.length, 3, 4)).toBeNull();
	});
});

describe("linesLabel", () => {
	it("names one line or a range", () => {
		expect(linesLabel("a.rs", 3, 3)).toBe("a.rs:3");
		expect(linesLabel("a.rs", 3, 9)).toBe("a.rs:3–9");
	});
});

describe("the status bar's facts", () => {
	it("finds line endings, and says when they are mixed", () => {
		expect(detectLineEnding("a\nb\n")).toEqual({ ending: "LF", mixed: false });
		expect(detectLineEnding("a\r\nb\r\n")).toEqual({ ending: "CRLF", mixed: false });
		expect(detectLineEnding("a\r\nb\r\nc\n")).toEqual({ ending: "CRLF", mixed: true });
		expect(detectLineEnding("a\r\nb\nc\n")).toEqual({ ending: "LF", mixed: true });
		expect(detectLineEnding("one line")).toEqual({ ending: "LF", mixed: false });
		expect(detectLineEnding("")).toEqual({ ending: "LF", mixed: false });
		expect(separatorOf("CRLF")).toBe("\r\n");
	});

	it("finds indentation from the steps between lines", () => {
		expect(detectIndentation("a {\n  b {\n    c\n  }\n}\n")).toEqual({ useTabs: false, size: 2 });
		expect(detectIndentation("def f():\n    if x:\n        y\n    z\n")).toEqual({
			useTabs: false,
			size: 4,
		});
		expect(detectIndentation("a {\n\tb {\n\t\tc\n\t}\n}\n")).toEqual({ useTabs: true, size: 0 });
		expect(detectIndentation("a {\r\n  b\r\n  c\r\n}\r\n")).toEqual({ useTabs: false, size: 2 });
		// A block comment's lone space is not a step.
		expect(detectIndentation("/**\n * one\n * two\n */\nfn {\n    a\n    b\n}\n")).toEqual({
			useTabs: false,
			size: 4,
		});
		expect(detectIndentation("no\nindentation\n")).toBeNull();
		expect(detectIndentation("")).toBeNull();
	});

	it("counts columns as shown, tabs to their stop", () => {
		expect(visualColumn("abc", 0, 4)).toBe(1);
		expect(visualColumn("abc", 2, 4)).toBe(3);
		expect(visualColumn("\tx", 1, 4)).toBe(5);
		expect(visualColumn("ab\tx", 3, 4)).toBe(5);
		expect(visualColumn("\tx", 1, 2)).toBe(3);
	});

	it("says it in the status bar's words", () => {
		expect(cursorLabel({ line: 10, column: 1, selected: 0, cursors: 1 })).toBe("Ln 10, Col 1");
		expect(cursorLabel({ line: 1, column: 4, selected: 12, cursors: 1 })).toBe(
			"Ln 1, Col 4 (12 selected)",
		);
		expect(cursorLabel({ line: 1, column: 4, selected: 3, cursors: 2 })).toBe(
			"Ln 1, Col 4 (2 selections)",
		);
		expect(indentationLabel(false, 2)).toBe("Spaces: 2");
		expect(indentationLabel(true, 4)).toBe("Tab Size: 4");
		expect(encodingOf("\uFEFFhello")).toBe("UTF-8 with BOM");
		expect(encodingOf("hello")).toBe("UTF-8");
	});
});

describe("problems in the editor", () => {
	const line = { from: 10, text: "  let value = 1;  " };
	it("underlines a range, else the word, else the line's text", () => {
		expect(problemRange(line, 12, 15)).toEqual({ from: 12, to: 15 });
		expect(problemRange(line, 18, 18)).toEqual({ from: 16, to: 21 });
		expect(problemRange(line, 16, 16)).toEqual({ from: 16, to: 21 });
		expect(problemRange(line, 10, 10)).toEqual({ from: 12, to: 26 });
		expect(problemRange({ from: 0, text: "" }, 0, 0)).toEqual({ from: 0, to: 0 });
	});

	it("finds the worst severity", () => {
		expect(worstSeverity(["hint", "warning", "info"])).toBe("warning");
		expect(worstSeverity(["hint", "error"])).toBe("error");
		expect(worstSeverity([])).toBeUndefined();
	});

	it("goes to the next and previous problem, round the file", () => {
		const problems = [{ from: 5 }, { from: 20 }, { from: 40 }];
		expect(nextProblemIndex(problems, 0, 1)).toBe(0);
		expect(nextProblemIndex(problems, 5, 1)).toBe(1);
		expect(nextProblemIndex(problems, 50, 1)).toBe(0);
		expect(nextProblemIndex(problems, 20, -1)).toBe(0);
		expect(nextProblemIndex(problems, 3, -1)).toBe(2);
		expect(nextProblemIndex([], 3, 1)).toBe(-1);
	});
});
