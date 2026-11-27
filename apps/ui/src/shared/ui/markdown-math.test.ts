import { describe, expect, it } from "vitest";

import { mayHoldMath, normalizeMath } from "@/shared/ui/markdown-math";

describe("normalizeMath", () => {
	it("turns \\( \\) and \\[ \\] into dollars", () => {
		expect(normalizeMath("Euler: \\(e^{i\\pi} + 1 = 0\\).")).toBe("Euler: $e^{i\\pi} + 1 = 0$.");
		expect(normalizeMath("Sum:\n\\[\n\\sum_{k=1}^n k\n\\]\nDone")).toBe(
			"Sum:\n\n$$\n\\sum_{k=1}^n k\n$$\n\nDone",
		);
	});

	it("leaves code alone", () => {
		const fenced = "```tex\n\\(x\\)\n```\nand \\(y\\)";
		expect(normalizeMath(fenced)).toBe("```tex\n\\(x\\)\n```\nand $y$");
		expect(normalizeMath("`\\(x\\)` and \\(y\\)")).toBe("`\\(x\\)` and $y$");
	});

	it("leaves text without them as it is", () => {
		const text = "It costs $5, or $10 with tax.";
		expect(normalizeMath(text)).toBe(text);
	});
});

describe("mayHoldMath", () => {
	it("is true for any of the delimiters", () => {
		expect(mayHoldMath("$x$")).toBe(true);
		expect(mayHoldMath("\\(x\\)")).toBe(true);
		expect(mayHoldMath("\\[x\\]")).toBe(true);
		expect(mayHoldMath("plain words")).toBe(false);
	});
});
