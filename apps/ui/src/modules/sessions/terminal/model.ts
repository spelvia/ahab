import type { Session } from "@/shared/api/generated/ahab.schemas";
import { forAhab as keymapForAhab } from "@/shared/keymap/model";
import { keymap } from "@/shared/keymap/store";
import { mac } from "@/shared/platform";

/**
 * The title a session shows: a terminal's shell title while the session
 * keeps the title it started with (it was not renamed), else its own.
 */
export function shownTitle(session: Pick<Session, "title" | "kind" | "terminal">): string {
	const terminal = session.kind === "terminal" ? session.terminal : null;
	if (!terminal?.title) return session.title;
	const started = terminal.command ?? "Terminal";
	return session.title === started ? terminal.title : session.title;
}

/** A path found in a terminal's line, with where it is and its line number. */
export interface FoundPath {
	start: number;
	end: number;
	path: string;
	line?: number;
}

// Paths with a folder (`src/main.rs`, `./a/b.ts`, `/abs/x.py`) or with a
// line number (`main.rs:42`), as compilers and tools print them.
const PATH = /(?:\.{0,2}\/)?(?:[\w.@+-]+\/)*[\w.@+-]+\.[A-Za-z0-9]+(?::(\d+)(?::\d+)?)?/g;

/**
 * The file paths in a line of a terminal's text that point into the
 * project (`root`): relative ones, and absolute ones inside it.
 */
export function findPaths(text: string, root?: string): FoundPath[] {
	const found: FoundPath[] = [];
	for (const match of text.matchAll(PATH)) {
		const whole = match[0];
		const start = match.index ?? 0;
		const before = text[start - 1];
		// Inside a word or an address (`https://x.y/a.js`): not a path.
		if (before && /[\w:/]/.test(before)) continue;
		const lineNumber = match[1] ? Number(match[1]) : undefined;
		const bare = whole.replace(/:\d+(?::\d+)?$/, "");
		const hasFolder = bare.includes("/");
		if (!hasFolder && lineNumber === undefined) continue;
		let path = bare.replace(/^\.\//, "");
		if (path.startsWith("/")) {
			if (!root) continue;
			const base = root.endsWith("/") ? root : `${root}/`;
			if (!path.startsWith(base)) continue;
			path = path.slice(base.length);
		}
		if (path.startsWith("../")) continue;
		found.push({ start, end: start + whole.length, path, line: lineNumber });
	}
	return found;
}

/** What Ctrl and a key send: `c` → ETX (^C), `[` → ESC. */
export function ctrlOf(key: string): string {
	if (key.length !== 1) return key;
	const code = key.toUpperCase().charCodeAt(0);
	return code >= 64 && code <= 95 ? String.fromCharCode(code - 64) : key;
}

/** Ahab's window keys with Alt (Windows, Linux), which shells rarely use. */
/**
 * Whether a key pressed in a terminal is Ahab's rather than the shell's,
 * by the keymap in use (see `forAhab` in `@/shared/keymap/model`).
 */
export function forAhab(
	event: Pick<KeyboardEvent, "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "code">,
	onMac = mac,
): boolean {
	return keymapForAhab(keymap(), event, onMac);
}
