import { useMemo, type ReactNode } from "react";

// Standard SGR foreground colours, using the app's theme for contrast.
const colours = [
	"text-muted-foreground",
	"text-destructive",
	"text-success",
	"text-warning",
	"text-primary",
	"text-purple-500",
	"text-cyan-600 dark:text-cyan-400",
	"text-foreground",
];

/** Removes terminal escapes (colours, cursor moves, titles), leaving the text. */
export function stripAnsi(text: string): string {
	return text.replace(
		// oxlint-disable-next-line no-control-regex -- terminal escapes start with ESC.
		/\u001b(?:\[[\d;?]*[ -/]*[@-~]|\][^\u0007\u001b]*(?:\u0007|\u001b\\)|[@-Z\\-_])/g,
		"",
	);
}

/** How much of the end of a long log keeps its colours; the rest is plain text. */
const COLOURED_TAIL = 200_000;

/**
 * How to render a log: as one plain text without escapes, or as a plain
 * head (escapes removed) and a coloured tail that starts at a line, so a
 * long log never becomes a span per colour change.
 */
export function logRendering(
	text: string,
	tail = COLOURED_TAIL,
): { mode: "plain"; text: string } | { mode: "ansi"; head: string; tail: string } {
	if (!text.includes("\u001b")) return { mode: "plain", text };
	if (text.length <= tail) return { mode: "ansi", head: "", tail: text };
	const line = text.indexOf("\n", text.length - tail);
	const cut = line < 0 ? text.length - tail : line + 1;
	return { mode: "ansi", head: stripAnsi(text.slice(0, cut)), tail: text.slice(cut) };
}

/** A long log: its colours parsed only near its end, and not at all without escapes. */
export function TerminalLog({ text }: { text: string }) {
	const shown = useMemo(() => logRendering(text), [text]);
	if (shown.mode === "plain") return <>{shown.text}</>;
	return (
		<>
			{shown.head}
			<TerminalOutput text={shown.tail} />
		</>
	);
}

/** Render native terminal colour without interpreting output as HTML. */
export function TerminalOutput({ text }: { text: string }) {
	return useMemo(() => {
		if (!text.includes("\u001b")) return text;
		const parts: ReactNode[] = [];
		let offset = 0;
		let foreground = "";
		let bold = false;
		const append = (end: number) => {
			if (end === offset) return;
			const chunk = text.slice(offset, end);
			parts.push(
				foreground || bold ? (
					<span key={offset} className={[foreground, bold ? "font-bold" : ""].join(" ")}>
						{chunk}
					</span>
				) : (
					chunk
				),
			);
		};
		// oxlint-disable-next-line no-control-regex -- ANSI SGR starts with ESC.
		for (const match of text.matchAll(/\u001b\[([\d;]*)m/g)) {
			append(match.index);
			const codes = (match[1] ?? "").split(";").map(Number);
			for (let i = 0; i < codes.length; i++) {
				const code = codes[i] ?? 0;
				if (code === 0) {
					foreground = "";
					bold = false;
				} else if (code === 1 || code === 22) bold = code === 1;
				else if (code === 39) foreground = "";
				else if (code >= 30 && code <= 37) foreground = colours[code - 30] ?? "";
				else if (code >= 90 && code <= 97) foreground = colours[code - 90] ?? "";
				else if (code === 38 || code === 48) i += codes[i + 1] === 2 ? 4 : 2;
			}
			offset = match.index + match[0].length;
		}
		append(text.length);
		return parts;
	}, [text]);
}
