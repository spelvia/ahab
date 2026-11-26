/**
 * Math in markdown (KaTeX): `$…$` inline and `$$…$$` on lines of their own,
 * and the `\(…\)` and `\[…\]` many agents write, turned into those. Code,
 * fenced or inline, is left as it is.
 */

/** Whether `text` may hold math, so KaTeX is worth loading. */
export function mayHoldMath(text: string): boolean {
	return /\$|\\\(|\\\[/.test(text);
}

/** `text` with `\(…\)` as `$…$` and `\[…\]` as `$$…$$`, outside code. */
export function normalizeMath(text: string): string {
	if (!/\\\(|\\\[/.test(text)) return text;
	// Fenced blocks and inline code are kept: odd pieces are code.
	return text
		.split(/(^(?:```|~~~)[^\n]*\n[\s\S]*?^(?:```|~~~)[ \t]*$|`[^`\n]+`)/m)
		.map((piece, at) =>
			at % 2 === 1
				? piece
				: piece
						.replace(/\\\[([\s\S]+?)\\\]/g, (_, math: string) => `\n$$\n${math.trim()}\n$$\n`)
						.replace(/\\\(([\s\S]+?)\\\)/g, (_, math: string) => `$${math.trim()}$`),
		)
		.join("");
}
