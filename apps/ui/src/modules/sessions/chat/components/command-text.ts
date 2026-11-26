/**
 * Reading the commands agents run: the script inside a shell wrapper
 * (`zsh -lc '…'`), a one-line summary for the row title, and what their
 * output is. Pure text work, no rendering.
 */

const OPERATORS = ";&|<>()";

/**
 * Reads one shell word from `start`, removing its quotes and escapes:
 * `'a'\''b'` is `a'b`. Undefined when a quote is left open.
 */
export function readWord(text: string, start: number): { value: string; end: number } | undefined {
	let value = "";
	let i = start;
	while (i < text.length) {
		const c = text[i] ?? "";
		if (/\s/.test(c) || OPERATORS.includes(c)) break;
		if (c === "'") {
			const close = text.indexOf("'", i + 1);
			if (close < 0) return undefined;
			value += text.slice(i + 1, close);
			i = close + 1;
		} else if (c === "$" && text[i + 1] === "'") {
			// ANSI-C quoting: $'…\n…'.
			i += 2;
			while (i < text.length && text[i] !== "'") {
				if (text[i] === "\\" && i + 1 < text.length) {
					const next = text[i + 1] ?? "";
					value +=
						({ n: "\n", t: "\t", r: "\r", "0": "\0" } as Record<string, string>)[next] ?? next;
					i += 2;
				} else value += text[i++];
			}
			if (i >= text.length) return undefined;
			i++;
		} else if (c === '"') {
			i++;
			while (i < text.length && text[i] !== '"') {
				const next = text[i + 1];
				if (text[i] === "\\" && next !== undefined && '"\\$`\n'.includes(next)) {
					if (next !== "\n") value += next;
					i += 2;
				} else value += text[i++];
			}
			if (i >= text.length) return undefined;
			i++;
		} else if (c === "\\") {
			const next = text[i + 1];
			if (next === undefined) return undefined;
			if (next !== "\n") value += next;
			i += 2;
		} else {
			value += c;
			i++;
		}
	}
	return { value, end: i };
}

const WRAPPER =
	/^\s*((?:\S*\/)?(?:env\s+)?(?:\S*\/)?(?:bash|zsh|sh|dash|ksh)(?:\s+-[a-zA-Z]+)*?\s+-[a-zA-Z]*c[a-zA-Z]*)\s+(?=\S)/;

/**
 * The script a shell wrapper runs (`bash -lc '…'`, quotes and escapes
 * removed), and the wrapper; the command itself when it has none.
 */
export function unwrapShell(command: string): { script: string; wrapper?: string } {
	const match = WRAPPER.exec(command);
	if (!match?.[1]) return { script: command };
	const word = readWord(command, match[0].length);
	if (!word || command.slice(word.end).trim() || !word.value.trim()) return { script: command };
	return { script: word.value, wrapper: match[1].trim() };
}

/**
 * One line for a command's row: the first line that does something
 * (comments and `set -e` skipped), with `…` when more follows.
 */
export function shellSummary(command: string): string {
	const lines = unwrapShell(command)
		.script.split("\n")
		.map((line) => line.trim())
		.filter(Boolean);
	const meaningful = lines.findIndex(
		(line) => !line.startsWith("#") && !/^set\s+[-+]\w+/.test(line),
	);
	const at = meaningful < 0 ? 0 : meaningful;
	const first = lines[at] ?? "";
	return at < lines.length - 1 ? `${first} …` : first;
}

/** Output highlighted when it is clearly structured, and small enough. */
const STRUCTURED_MAX = 200_000;

/** How to show a command's output: as JSON, as a unified diff, or as terminal text. */
export function outputKind(output: string): "json" | "diff" | "terminal" {
	if (output.length > STRUCTURED_MAX || output.includes("\u001b")) return "terminal";
	const trimmed = output.trim();
	if (/^[[{]/.test(trimmed)) {
		try {
			JSON.parse(trimmed);
			return "json";
		} catch {
			// Not JSON after all.
		}
	}
	if (
		/^diff --git /m.test(output) ||
		(/^--- \S/m.test(output) && /^\+\+\+ \S/m.test(output) && /^@@ -\d/m.test(output))
	)
		return "diff";
	return "terminal";
}

/** The one file a unified diff changes, when it changes just one. */
export function diffPath(diff: string): string | undefined {
	const paths = new Set(
		Array.from(diff.matchAll(/^\+\+\+ (?:b\/)?(\S+)/gm), (match) => match[1]).filter(
			(path) => path && path !== "/dev/null",
		),
	);
	return paths.size === 1 ? [...paths][0] : undefined;
}
