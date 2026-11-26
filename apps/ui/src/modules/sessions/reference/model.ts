/**
 * Context references as a sent message carries them: the server
 * writes each after the message's text as a `Context:` line and, unless it
 * is a whole file, the text in a fenced block
 * (`crates/ahab/src/modules/sessions/reference/model.rs`). The conversation
 * reads them back to show chips instead of the raw blocks.
 */

import type { ReferenceKind } from "@/shared/chat-context/model";

const PREFIX = "Context: ";
const STALE = " (changed on disk since it was selected)";

/** A reference read back from a sent message. */
export interface SentReference {
	kind: ReferenceKind | "other";
	path?: string;
	startLine?: number;
	endLine?: number;
	label?: string;
	/** Its file changed between selecting and sending. */
	stale: boolean;
	/** The text as it was sent, when it was. */
	text?: string;
}

/** A message's own text, and the references after it. */
export function splitReferences(message: string): { text: string; references: SentReference[] } {
	const starts: number[] = message.startsWith(PREFIX) ? [0] : [];
	for (
		let at = message.indexOf(`\n\n${PREFIX}`);
		at !== -1;
		at = message.indexOf(`\n\n${PREFIX}`, at + 1)
	)
		starts.push(at + 2);
	for (const start of starts) {
		const references = blocks(message.slice(start));
		if (references) return { text: message.slice(0, start).trimEnd(), references };
	}
	return { text: message, references: [] };
}

/** `rest` read as references and nothing else, or null. */
function blocks(rest: string): SentReference[] | null {
	const found: SentReference[] = [];
	let at = 0;
	while (at < rest.length) {
		if (!rest.startsWith(PREFIX, at)) return null;
		const end = rest.indexOf("\n", at);
		const heading = rest.slice(at + PREFIX.length, end === -1 ? undefined : end);
		// Where the reference ends: its line, or its block's closing fence.
		let next = end === -1 ? rest.length : end;
		let text: string | undefined;
		const opening = end === -1 ? null : /^(`{3,})[^\n`]*\n/.exec(rest.slice(end + 1));
		if (opening?.[1]) {
			const fence = opening[1];
			const body = end + 1 + opening[0].length;
			let close = rest.indexOf(`\n${fence}`, body - 1);
			// The closing fence stands alone: the end, or a blank line after.
			while (close !== -1) {
				const after = close + 1 + fence.length;
				if (after === rest.length || rest.startsWith("\n\n", after)) break;
				close = rest.indexOf(`\n${fence}`, close + 1);
			}
			if (close === -1) return null;
			text = close < body ? "" : rest.slice(body, close);
			next = close + 1 + fence.length;
		}
		found.push({ ...meaning(heading), text });
		if (next >= rest.length) break;
		if (!rest.startsWith("\n\n", next)) return null;
		at = next + 2;
	}
	return found;
}

function lines(text: string): { path: string; startLine?: number; endLine?: number } {
	const range = /^(.*), lines (\d+)–(\d+)$/.exec(text);
	if (range?.[1] !== undefined)
		return { path: range[1], startLine: Number(range[2]), endLine: Number(range[3]) };
	const one = /^(.*), line (\d+)$/.exec(text);
	if (one?.[1] !== undefined)
		return { path: one[1], startLine: Number(one[2]), endLine: Number(one[2]) };
	return { path: text };
}

/** What a `Context:` line says. */
function meaning(line: string): SentReference {
	const stale = line.endsWith(STALE);
	const what = stale ? line.slice(0, -STALE.length) : line;
	if (what.startsWith("the diff of "))
		return { kind: "diffHunk", stale, ...lines(what.slice("the diff of ".length)) };
	if (what === "terminal output") return { kind: "terminalOutput", stale };
	if (what.startsWith("terminal output from "))
		return { kind: "terminalOutput", stale, label: what.slice("terminal output from ".length) };
	if (what === "search results") return { kind: "searchResults", stale };
	if (what.startsWith("search results for "))
		return { kind: "searchResults", stale, label: what.slice("search results for ".length) };
	if (what === "chat history") return { kind: "chatHistory", stale };
	if (what.startsWith("chat history from "))
		return { kind: "chatHistory", stale, label: what.slice("chat history from ".length) };
	if (what.endsWith(" (the whole file)"))
		return { kind: "file", stale, path: what.slice(0, -" (the whole file)".length) };
	return { kind: "lines", stale, ...lines(what) };
}
