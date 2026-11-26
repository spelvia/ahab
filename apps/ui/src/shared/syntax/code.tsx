/**
 * Highlighted code outside the editor: code blocks in
 * chats and Markdown, diffs, search results. The same engine and theme as
 * the editor, so code looks the same wherever it appears. Text shows plain
 * at once and takes its colours when the engine answers.
 */

import { useEffect, useMemo, useState } from "react";

import type { HighlightToken } from "@/shared/editor/model";
import { loadSyntax } from "@/shared/syntax/load";
import { type Span, spansOf, tokenClasses, tokensByLine } from "@/shared/syntax/model";

/** What to highlight a text as: a language by ID or name (`ts`), or the language of a path. */
export type HighlightAs = { language: string } | { path: string };

/** A text's tokens, once highlighted; null while waiting, for plain text, or without `as`. */
function useHighlight(
	text: string | undefined,
	as: HighlightAs | undefined,
): HighlightToken[] | null {
	const key = as ? ("language" in as ? `l:${as.language}` : `p:${as.path}`) : "";
	const [found, setFound] = useState<{
		text: string;
		key: string;
		tokens: HighlightToken[] | null;
	}>();
	useEffect(() => {
		if (!as || text === undefined || !text) return;
		let live = true;
		void loadSyntax()
			.then((syntax) => syntax.highlight(text, as))
			.then((tokens) => {
				if (live) setFound({ text, key, tokens });
			})
			.catch(() => {
				// Plain text, then.
			});
		return () => {
			live = false;
		};
		// `key` stands for `as`.
		// oxlint-disable-next-line react-hooks/exhaustive-deps
	}, [text, key]);
	return found && found.text === text && found.key === key ? found.tokens : null;
}

/** A text's tokens per line (from 0), relative to each line. */
export function useHighlightedLines(
	text: string | undefined,
	as: HighlightAs | undefined,
): HighlightToken[][] | null {
	const tokens = useHighlight(text, as);
	return useMemo(
		() => (tokens && text !== undefined ? tokensByLine(text, tokens) : null),
		[tokens, text],
	);
}

/** Spans as elements: each highlighted one with its capture's classes, marked ones with `markClass`. */
function renderSpans(spans: Span[], markClass?: string): React.ReactNode[] {
	return spans.map((span, index) => {
		const classes = [
			span.tag ? tokenClasses(span.tag) : "",
			span.marked && markClass ? markClass : "",
		]
			.filter(Boolean)
			.join(" ");
		return (
			// The spans of a text never reorder.
			// oxlint-disable-next-line react/no-array-index-key
			<span key={index} className={classes || undefined}>
				{span.text}
			</span>
		);
	});
}

/** A line's text with its tokens and, optionally, marked ranges (a diff's changed words). */
export function HighlightedLine({
	text,
	tokens,
	marks,
	markClass,
}: {
	text: string;
	tokens?: HighlightToken[];
	marks?: { from: number; to: number }[];
	markClass?: string;
}) {
	if (!tokens?.length && !marks?.length) return <>{text}</>;
	return <>{renderSpans(spansOf(text, tokens, marks), markClass)}</>;
}

/** Code highlighted as a whole: a code block's content. */
export function HighlightedCode({ text, as }: { text: string; as: HighlightAs | undefined }) {
	const tokens = useHighlight(text, as);
	if (!tokens?.length) return <>{text}</>;
	return <>{renderSpans(spansOf(text, tokens))}</>;
}

/**
 * Tokens for the lines of a unified diff (`+`, `-` and ` ` lines), each
 * relative to the line after its first character: the old and new sides
 * are put together and highlighted as the file's language. Other lines
 * (headers, `@@`) get none.
 */
export function useUnifiedDiffTokens(
	diff: string,
	path: string | undefined,
): HighlightToken[][] | null {
	const sides = useMemo(() => {
		const lines = diff.split("\n");
		const old: string[] = [];
		const now: string[] = [];
		/** For each diff line: which side and which line of it. */
		const at = lines.map((line) => {
			if (line.startsWith("+++") || line.startsWith("---")) return null;
			const body = line.slice(1);
			if (line.startsWith("+")) return { side: "new" as const, line: now.push(body) - 1 };
			if (line.startsWith("-")) return { side: "old" as const, line: old.push(body) - 1 };
			if (line.startsWith(" ")) {
				old.push(body);
				return { side: "new" as const, line: now.push(body) - 1 };
			}
			return null;
		});
		return { old: old.join("\n"), now: now.join("\n"), at };
	}, [diff]);
	const as = path ? { path } : undefined;
	const oldLines = useHighlightedLines(sides.old, as);
	const newLines = useHighlightedLines(sides.now, as);
	return useMemo(() => {
		if (!oldLines && !newLines) return null;
		return sides.at.map((place) => {
			if (!place) return [];
			return (place.side === "old" ? oldLines : newLines)?.[place.line] ?? [];
		});
	}, [sides, oldLines, newLines]);
}
