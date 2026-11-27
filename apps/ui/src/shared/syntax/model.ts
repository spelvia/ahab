/**
 * Languages and what the syntax engine gives back, in plain
 * terms that know nothing of Tree-sitter: a language's declaration (the
 * same for bundled languages and plugins'), how a file finds its
 * language, and helpers for tokens. The engine itself runs in a worker
 * (`worker.ts`); `client.ts` talks to it.
 */

import type { DocumentSymbol, HighlightToken } from "@/shared/editor/model";

/**
 * A language as declared: by Ahab (`apps/ui/languages/languages.json`) or in a
 * plugin's manifest (`contributes.languages`). Paths are relative to where
 * the declaration lives. Language tooling adds its tools to the same declaration;
 * fields this version does not know are kept.
 */
export interface LanguageDeclaration {
	/** Lowercase, such as `rust`; a plugin's language with a bundled ID replaces it. */
	id: string;
	name: string;
	files?: {
		/** Without the dot: `rs`. */
		extensions?: string[];
		/** Exact names (`Dockerfile`), or with `*` (`Dockerfile.*`). */
		names?: string[];
		/** Interpreters named by `#!`: `python3`, `bash`. */
		shebangs?: string[];
	};
	/** Other names, as code fences and injections write them (`py`, `sh`). */
	aliases?: string[];
	comments?: { line?: string; block?: [string, string] };
	/** The grammar: Tree-sitter WebAssembly, gzipped or not. None: plain text. */
	grammar?: { wasm: string };
	/** Tree-sitter queries (`.scm`), each optional. */
	queries?: {
		highlights?: string;
		injections?: string;
		/** Definitions and references, for the outline and the approximate navigation. */
		tags?: string;
		/** Folding; without it, folding follows the syntax tree's blocks. */
		folds?: string;
	};
	/** Only inside other languages (`markdown.inline`): not offered to choose. */
	hidden?: boolean;
	[more: string]: unknown;
}

/** A language ready to load: its declaration with absolute addresses. */
export interface Language {
	id: string;
	name: string;
	declaration: LanguageDeclaration;
	/** Where it comes from: `bundled`, or the plugin's ID. */
	source: string;
	/** The grammar's address; none for a language without one. */
	wasm?: string;
	queries: Partial<Record<QueryKind, string>>;
}

export type QueryKind = "highlights" | "injections" | "tags" | "folds";
export const QUERY_KINDS: QueryKind[] = ["highlights", "injections", "tags", "folds"];

/** Plain text: the language of a file nothing else claims. */
export const PLAIN_TEXT = "plaintext";

/** Files over this, or with a line over {@link MAX_LINE}, are not highlighted. */
const MAX_SIZE = 2 * 1024 * 1024;
export const MAX_LINE = 10_000;

/** Why a text is not highlighted, or null when it may be. */
export function tooLarge(text: string): "size" | "line" | null {
	if (text.length > MAX_SIZE) return "size";
	let start = 0;
	while (start < text.length) {
		const end = text.indexOf("\n", start);
		const stop = end === -1 ? text.length : end;
		if (stop - start > MAX_LINE) return "line";
		start = stop + 1;
	}
	return null;
}

/** A declaration's paths made absolute against `base` (a folder's address, ending in `/`). */
export function resolveLanguage(
	declaration: LanguageDeclaration,
	base: string,
	source: string,
	url: (path: string) => string = (path) => new URL(path, base).href,
): Language {
	const queries: Partial<Record<QueryKind, string>> = {};
	for (const kind of QUERY_KINDS) {
		const path = declaration.queries?.[kind];
		if (typeof path === "string" && path) queries[kind] = url(path);
	}
	return {
		id: declaration.id,
		name: declaration.name,
		declaration,
		source,
		wasm: declaration.grammar?.wasm ? url(declaration.grammar.wasm) : undefined,
		queries,
	};
}

/** Whether `value` reads as a declaration: what a plugin's manifest must give at least. */
export function isDeclaration(value: unknown): value is LanguageDeclaration {
	if (!value || typeof value !== "object") return false;
	const item = value as Record<string, unknown>;
	return (
		typeof item.id === "string" &&
		/^[a-z][a-z0-9.+#_-]*$/.test(item.id) &&
		typeof item.name === "string"
	);
}

// --- Finding a file's language ---------------------------------------------------

function nameMatches(pattern: string, name: string): boolean {
	if (!pattern.includes("*")) return pattern === name;
	const expression = pattern
		.split("*")
		.map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&"))
		.join(".*");
	return new RegExp(`^${expression}$`).test(name);
}

/** The interpreter a `#!` line names: `#!/usr/bin/env -S python3 -u` → `python3`. */
export function shebangOf(firstLine: string): string | undefined {
	const match = /^#!\s*(\S+)(.*)$/.exec(firstLine);
	if (!match) return undefined;
	const [, program = "", rest = ""] = match;
	const base = program.split("/").at(-1) ?? "";
	if (base !== "env") return base;
	const args = rest.trim().split(/\s+/);
	return args.find((arg) => !arg.startsWith("-") && !arg.includes("="));
}

/** What an editor modeline names: `vim: set ft=python:` or `-*- mode: python -*-`. */
export function modelineOf(text: string): string | undefined {
	// The first and last five lines, without splitting a large text.
	const head = text.slice(0, 2000).split("\n").slice(0, 5);
	const tail = text.length > 2000 ? text.slice(-2000).split("\n").slice(-5) : [];
	for (const line of [...head, ...tail]) {
		const vim = /\b(?:vi|vim|ex):.*\b(?:ft|filetype|syntax)=([\w.+-]+)/.exec(line);
		if (vim?.[1]) return vim[1];
		const emacs =
			/-\*-.*\bmode:\s*([\w.+-]+).*-\*-/.exec(line) ?? /-\*-\s*([\w.+-]+)\s*-\*-/.exec(line);
		if (emacs?.[1]) return emacs[1];
	}
	return undefined;
}

/** A language by ID, alias or extension, as code fences and injections name them; any case. */
export function languageNamed(languages: readonly Language[], name: string): Language | undefined {
	const wanted = name.trim().toLowerCase();
	if (!wanted) return undefined;
	const lists = (language: Language) => language.declaration;
	return (
		languages.find((language) => language.id === wanted) ??
		languages.find((language) =>
			lists(language).aliases?.some((alias) => alias.toLowerCase() === wanted),
		) ??
		languages.find((language) =>
			lists(language).files?.extensions?.some((extension) => extension.toLowerCase() === wanted),
		) ??
		languages.find((language) => language.name.toLowerCase() === wanted)
	);
}

/**
 * A file's language: by extension (the longest that fits:
 * `d.ts` before `ts`), exact name, the interpreter its `#!` names, then an
 * editor modeline. Later languages in `languages` win over earlier ones
 * (a plugin's over a bundled one). `text` is the file's text, when known.
 */
export function detectLanguage(
	languages: readonly Language[],
	path: string,
	text?: string,
): Language | undefined {
	const name = path.split("/").at(-1) ?? path;
	const ordered = languages.toReversed().filter((language) => !language.declaration.hidden);
	const lower = name.toLowerCase();
	let best: { language: Language; length: number } | undefined;
	for (const language of ordered) {
		for (const extension of language.declaration.files?.extensions ?? []) {
			const suffix = `.${extension.toLowerCase()}`;
			// A dot file (`.bashrc`) has no extension; `.bashrc` is a name.
			if (
				lower.endsWith(suffix) &&
				lower.length > suffix.length &&
				(!best || suffix.length > best.length)
			)
				best = { language, length: suffix.length };
		}
	}
	if (best) return best.language;
	const byName = ordered.find((language) =>
		language.declaration.files?.names?.some((pattern) => nameMatches(pattern, name)),
	);
	if (byName) return byName;
	if (text === undefined) return undefined;
	const program = shebangOf(
		text.slice(0, text.indexOf("\n") === -1 ? undefined : text.indexOf("\n")),
	);
	if (program) {
		const bare = program.replace(/[\d.]+$/, "");
		const found = ordered.find((language) =>
			language.declaration.files?.shebangs?.some((item) => item === program || item === bare),
		);
		if (found) return found;
	}
	const mode = modelineOf(text);
	return mode ? languageNamed(ordered, mode) : undefined;
}

/** The pattern a choice "for every such file" uses: `*.h`, or the name itself (`Makefile`). */
export function patternOf(path: string): string {
	const name = path.split("/").at(-1) ?? path;
	const dot = name.lastIndexOf(".");
	return dot > 0 ? `*${name.slice(dot)}` : name;
}

// --- Tokens -------------------------------------------------------------------------

/** The classes a capture shows as: `function.call` → `tok-function tok-function-call`. */
export function tokenClasses(tag: string): string {
	const parts = tag.split(".");
	return parts.map((_, index) => `tok-${parts.slice(0, index + 1).join("-")}`).join(" ");
}

/** A piece of a line: its text and, when highlighted, its capture. */
export interface Span {
	text: string;
	tag?: string;
	/** Marked by the caller (a changed word in a diff). */
	marked?: boolean;
}

/**
 * Tokens of a whole text split by line: for each line (from 0) its tokens,
 * relative to the line's start and cut at its end.
 */
export function tokensByLine(text: string, tokens: readonly HighlightToken[]): HighlightToken[][] {
	const starts = [0];
	for (let index = text.indexOf("\n"); index !== -1; index = text.indexOf("\n", index + 1))
		starts.push(index + 1);
	const lines: HighlightToken[][] = starts.map(() => []);
	const lineOf = (offset: number) => {
		let low = 0;
		let high = starts.length - 1;
		while (low < high) {
			const middle = (low + high + 1) >> 1;
			if ((starts[middle] ?? 0) <= offset) low = middle;
			else high = middle - 1;
		}
		return low;
	};
	for (const token of tokens) {
		for (let line = lineOf(token.from); line < starts.length; line++) {
			const start = starts[line] ?? 0;
			if (start >= token.to) break;
			const end = line + 1 < starts.length ? (starts[line + 1] ?? 0) - 1 : text.length;
			const from = Math.max(token.from, start) - start;
			const to = Math.min(token.to, end) - start;
			if (to > from) lines[line]?.push({ from, to, tag: token.tag });
		}
	}
	return lines;
}

/**
 * A line cut into spans by its tokens and, optionally, marked ranges (a
 * diff's changed words): each span one tag and one mark.
 */
export function spansOf(
	text: string,
	tokens: readonly HighlightToken[] = [],
	marks: readonly { from: number; to: number }[] = [],
): Span[] {
	const cuts = new Set([0, text.length]);
	for (const item of [...tokens, ...marks]) {
		cuts.add(Math.max(0, Math.min(item.from, text.length)));
		cuts.add(Math.max(0, Math.min(item.to, text.length)));
	}
	const points = [...cuts].toSorted((a, b) => a - b);
	const spans: Span[] = [];
	for (let index = 0; index + 1 < points.length; index++) {
		const from = points[index] ?? 0;
		const to = points[index + 1] ?? 0;
		if (to <= from) continue;
		// The innermost token wins: the last one covering this piece.
		const tag = tokens.findLast((token) => token.from <= from && token.to >= to)?.tag;
		const marked = marks.some((mark) => mark.from <= from && mark.to >= to);
		const last = spans.at(-1);
		if (last && last.tag === tag && Boolean(last.marked) === marked)
			last.text += text.slice(from, to);
		else
			spans.push({
				text: text.slice(from, to),
				...(tag ? { tag } : {}),
				...(marked ? { marked } : {}),
			});
	}
	return spans;
}

// --- Structure (slice 4) ------------------------------------------------------------

/** A name defined or used, as a language's tags query finds it. */
export interface Tag {
	name: string;
	/** `definition` or `reference`. */
	role: "definition" | "reference";
	/** `function`, `class`, `call`…: what the tags query says after the role. */
	kind: string;
	/** The whole definition or use. */
	from: number;
	to: number;
	/** The name in it. */
	nameFrom: number;
	nameTo: number;
	/** The name's line, from 1. */
	line: number;
}

/** Folds definitions into a tree by containment: the outline. */
export function nestSymbols(flat: readonly Omit<DocumentSymbol, "children">[]): DocumentSymbol[] {
	const sorted = flat.toSorted((a, b) => a.from - b.from || b.to - a.to);
	const roots: DocumentSymbol[] = [];
	const stack: DocumentSymbol[] = [];
	for (const item of sorted) {
		const symbol: DocumentSymbol = { ...item, children: [] };
		while (stack.length && (stack.at(-1)?.to ?? 0) < symbol.to) stack.pop();
		const parent = stack.at(-1);
		if (parent && parent.from <= symbol.from && parent.to >= symbol.to)
			parent.children.push(symbol);
		else roots.push(symbol);
		stack.push(symbol);
	}
	return roots;
}
