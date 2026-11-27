/**
 * The editor's own terms, independent of CodeMirror: what a
 * window shows and reports, and the seams later work plugs into, so the
 * editor underneath could be replaced. Positions are offsets into the
 * document's text (UTF-16 code units, as JavaScript strings count), with
 * helpers for line and character pairs where a protocol wants them (LSP).
 */

import { t } from "@/shared/i18n";

/** Whole lines of a selection: partial lines count as whole lines. */
export interface LineSelection {
	/** From 1. */
	startLine: number;
	/** Included. */
	endLine: number;
	/** The lines' text, as shown. */
	text: string;
}

/** What the editor's look follows: Settings → Editor. */
export interface EditorSettings {
	fontSize: number;
	lineWrapping: boolean;
	lineNumbers: boolean;
	indentationGuides: boolean;
	tabSize: number;
	/** Indent with tabs (EditorConfig's `indent_style = tab`). */
	useTabs?: boolean;
	vim: boolean;
	/** Each line's worst problem shown at its end (Error Lens's way); off by default. */
	problemsAtLineEnd?: boolean;
}

export const DEFAULT_SETTINGS: EditorSettings = {
	fontSize: 13,
	lineWrapping: false,
	lineNumbers: true,
	indentationGuides: true,
	tabSize: 4,
	vim: false,
};

/** @public A line and the character in it, both from 0 (as LSP counts). */
export interface Position {
	line: number;
	character: number;
}

/** The text of a document, as the seams below read it. */
export interface EditorDocument {
	/** The file, relative to its project. */
	readonly path: string;
	/** Its text now. */
	text(): string;
	/** Changes with every edit. */
	readonly version: number;
	/**
	 * The language its window shows it as, when the window says:
	 * one chosen by hand or found by the window, or `plaintext`. Absent, an
	 * engine finds the language from the path and text itself.
	 */
	readonly languageId?: string;
	positionAt(offset: number): Position;
	offsetAt(position: Position): number;
}

/** One edit, in the coordinates of the text before it. */
export interface TextEdit {
	from: number;
	to: number;
	insert: string;
}

/** The edits that took a document from one version to the next. */
export interface DocumentChange {
	version: number;
	edits: TextEdit[];
}

// ---------------------------------------------------------------------------
// Highlighting from an external engine (Tree-sitter in a worker).
// The editor itself knows no language: until an engine is registered, text
// is plain.

/** A highlighted range: a capture name such as `keyword` or `function.call`. */
export interface HighlightToken {
	from: number;
	to: number;
	/** Shown as the classes `tok-function tok-function-call`. */
	tag: string;
}

/** A definition in a document, for the outline. */
export interface DocumentSymbol {
	name: string;
	/** `function`, `class`, `method`, `section`…, as the source names it. */
	kind: string;
	/** The whole definition, as offsets. */
	from: number;
	to: number;
	/** Its name. */
	nameFrom: number;
	nameTo: number;
	children: DocumentSymbol[];
}

/** A range of offsets. */
export interface TextRange {
	from: number;
	to: number;
}

/** Highlighting one open document. */
export interface HighlightSession {
	/** The document changed; tokens for the new version follow by `emit`. */
	update(change: DocumentChange): void;
	dispose(): void;
	/** The outline, from the syntax tree, for the version last sent. */
	symbols?(): Promise<DocumentSymbol[]>;
	/**
	 * The syntax nodes around `from`–`to`, innermost first, each
	 * larger than the one before (expanding the selection).
	 */
	selectionRanges?(from: number, to: number): Promise<TextRange[]>;
	/** How the language writes comments, for commenting lines out (none: not known yet). */
	comments?(): CommentTokens | undefined;
}

/** How a language writes comments, as its declaration says. */
export interface CommentTokens {
	line?: string;
	block?: [string, string];
}

/**
 * Where the document folds at `version`: each range from the end
 * of a fold's first line to where it ends. Replaces the indentation's folds.
 */
export type EmitFolds = (folds: TextRange[], at: { version: number }) => void;

/** Tokens for the document at `version`, for `range` (the whole of it when absent). */
export type EmitTokens = (
	tokens: HighlightToken[],
	at: { version: number; range?: { from: number; to: number } },
) => void;

/** An engine that highlights documents, such as the Tree-sitter worker. */
export interface Highlighter {
	/** Starts highlighting `document`, or answers null for a language it does not know. */
	open(document: EditorDocument, emit: EmitTokens, folds?: EmitFolds): HighlightSession | null;
}

// ---------------------------------------------------------------------------
// Language tooling (language servers through the daemon). Each part
// is optional; the editor offers what a service has, read-only too (hover,
// definition, references, diagnostics) and in Edit mode (completion,
// formatting).

/** @public An extension point type. */
export type Severity = "error" | "warning" | "info" | "hint";

/** @public LSP's diagnostic tags: code never used (faded), or deprecated (struck through). */
export type DiagnosticTag = "unnecessary" | "deprecated";

/** @public An extension point type. */
export interface Diagnostic {
	from: number;
	to: number;
	severity: Severity;
	message: string;
	/** The tool that said it, such as `rust-analyzer`. */
	source?: string;
	/** The rule's name or number (`E0308`, `no-unused-vars`). */
	code?: string;
	/** Where the rule is explained. */
	codeHref?: string;
	tags?: DiagnosticTag[];
}

const SEVERITY_RANK: Record<Severity, number> = { error: 0, warning: 1, info: 2, hint: 3 };

/** The most severe of some severities; none for none. */
export function worstSeverity(severities: Iterable<Severity>): Severity | undefined {
	let worst: Severity | undefined;
	for (const severity of severities)
		if (!worst || SEVERITY_RANK[severity] < SEVERITY_RANK[worst]) worst = severity;
	return worst;
}

/**
 * Where a problem is underlined, as VS Code does: its range; for an empty
 * range, the word there, else the line's text (without its indentation);
 * an empty line keeps the point. `line` is the line the problem starts on.
 */
export function problemRange(
	line: { from: number; text: string },
	from: number,
	to: number,
): TextRange {
	if (to > from) return { from, to };
	const at = from - line.from;
	const word = /[\p{L}\p{N}_$]/u;
	let start = at;
	let end = at;
	while (start > 0 && word.test(line.text[start - 1] ?? "")) start--;
	while (end < line.text.length && word.test(line.text[end] ?? "")) end++;
	if (end > start) return { from: line.from + start, to: line.from + end };
	const text = line.text.trimEnd();
	const indent = text.length - text.trimStart().length;
	if (text.length > indent) return { from: line.from + indent, to: line.from + text.length };
	return { from, to };
}

/**
 * The problem F8 goes to from `head`: the next one starting after it
 * (`1`), or the last one starting before it (`-1`), going round the file.
 * Problems in order of `from`; -1 when there are none.
 */
export function nextProblemIndex(
	problems: readonly { from: number }[],
	head: number,
	direction: 1 | -1,
): number {
	if (!problems.length) return -1;
	if (direction === 1) {
		const found = problems.findIndex((problem) => problem.from > head);
		return found === -1 ? 0 : found;
	}
	for (let index = problems.length - 1; index >= 0; index--)
		if ((problems[index]?.from ?? 0) < head) return index;
	return problems.length - 1;
}

/** @public An extension point type. */
export interface Hover {
	from: number;
	to: number;
	/** Shown as Markdown. */
	markdown: string;
}

/** A place in a project's file. */
export interface Location {
	/**
	 * Relative to the project; `external:<token>` for a file outside it a
	 * language server pointed at, opened read-only.
	 */
	path: string;
	/** From 1. */
	line: number;
	/** From 0. */
	character?: number;
	/** Found by name from the syntax tree, not by a language server. */
	approximate?: boolean;
	/** Where an external file comes from: "rust core", "node_modules/react". */
	origin?: string;
	/** An external file's name, for its tab. */
	name?: string;
}

/** A line and character range, as LSP counts (both from 0). */
export interface LineRange {
	start: Position;
	end: Position;
}

/** Edits to one file, in LSP's coordinates (the file may not be open). */
export interface FileEdits {
	path: string;
	edits: { range: LineRange; newText: string }[];
}

/** What serves a file, for its window's status item. */
export interface LanguageStatus {
	/** `Rust ✓`, `Rust · starting…`, `Rust · 2 issues`, `Java · basic`, `Markdown`. */
	label: string;
	tone: "ok" | "busy" | "warning" | "plain";
	/** The popover's heading: the language, and its root. */
	title: string;
	root?: string;
	rows: { name: string; ok: boolean | null; detail: string }[];
	notes: string[];
	actions: { label: string; run?: () => void; to?: string }[];
}

/** A function's signature while its arguments are typed. */
export interface SignatureHelp {
	label: string;
	/** The active parameter's place in the label. */
	active?: [number, number];
	documentation?: string;
}

/** A fix or refactoring a tool offers. */
export interface CodeAction {
	title: string;
	kind?: string;
	preferred?: boolean;
	/** Where it comes from (`eslint`). */
	source?: string;
	/** Its edits, resolved; applying them is the caller's (Edit mode). */
	resolve(): Promise<FileEdits[]>;
}

/** @public An extension point type. */
export interface CompletionItem {
	label: string;
	detail?: string;
	/** More about it, shown beside the list. */
	info?: string;
	/** What to insert, when not the label. */
	insert?: string;
	/** `function`, `variable`, `keyword`…, for its icon. */
	kind?: string;
}

/** @public An extension point type. */
export interface Completions {
	/** Where the word being completed starts. */
	from: number;
	items: CompletionItem[];
}

/** One document's language tooling. */
export interface LanguageService {
	/** Diagnostics as they come; returns a way to stop listening. */
	diagnostics?(listen: (items: Diagnostic[]) => void): () => void;
	/** What serves the file, as it changes; returns a way to stop. */
	status?(listen: (status: LanguageStatus | null) => void): () => void;
	hover?(offset: number): Promise<Hover | null>;
	definition?(offset: number): Promise<Location[]>;
	references?(offset: number): Promise<Location[]>;
	typeDefinition?(offset: number): Promise<Location[]>;
	implementation?(offset: number): Promise<Location[]>;
	/** The same symbol's other places in the document. */
	highlights?(offset: number): Promise<TextRange[]>;
	/** Edit mode only. */
	signatureHelp?(offset: number): Promise<SignatureHelp | null>;
	/** Edit mode only: the symbol at `offset` and its name, when it can be renamed. */
	prepareRename?(offset: number): Promise<{ from: number; to: number; name: string } | null>;
	/** Edit mode only: every file's edits that rename it, to preview first. */
	rename?(offset: number, name: string): Promise<FileEdits[]>;
	/** Edit mode only: fixes and refactorings for a range. */
	codeActions?(from: number, to: number): Promise<CodeAction[]>;
	/** Makes edits: this document's in its editor, others' on disk (versioned). */
	applyEdits?(edits: FileEdits[]): Promise<void>;
	/** Whether the file is formatted before it is saved. */
	formatOnSave?(): boolean;
	/** The file was saved (linters and checks that run on save). */
	saved?(): void;
	/** Edit mode only. */
	completion?(offset: number, explicit: boolean): Promise<Completions | null>;
	/** Edit mode only: the edits that format the document. */
	format?(): Promise<TextEdit[]>;
	/** The outline (LSP's `documentSymbol`); preferred to the syntax tree's. */
	symbols?(): Promise<DocumentSymbol[]>;
	/** The document changed (Edit mode). */
	update?(change: DocumentChange): void;
	dispose?(): void;
}

/** Which file a service is asked for. */
export interface LanguageFile {
	projectId: string;
	document: EditorDocument;
	readOnly: boolean;
	/** Changes this document in its editor (edits from tools: formatting, fixes, renames). */
	edit?(edits: TextEdit[]): void;
	/** Its indentation, for formatters (the editor's settings and EditorConfig). */
	indent?(): { tabSize: number; insertSpaces: boolean };
}

/** A file outside the project, as a language server granted it. */
export interface ExternalFile {
	text: string;
	name: string;
	/** Where it comes from: "rust core", "node_modules/react". */
	origin: string;
	/** Its full path or address. */
	display: string;
}

/** Which kind of places a navigation found. */
export type LocationKind = "definition" | "references" | "typeDefinition" | "implementation";

/** F2: renaming the symbol at the cursor, for the window to ask and preview. */
export interface RenameRequest {
	name: string;
	rename(name: string): Promise<FileEdits[]>;
	apply(edits: FileEdits[]): Promise<void>;
}

/** ⌘.: the fixes offered at the cursor, for the window to list. */
export interface CodeActionRequest {
	actions: CodeAction[];
	apply(edits: FileEdits[]): Promise<void>;
}

/** Gives a document its tooling, or null when it has none. */
export type LanguageProvider = (file: LanguageFile) => LanguageService | null;

// ---------------------------------------------------------------------------

/** The lines of a document a selection needs, as `lineSelections` reads them. */
export interface LineSource {
	lineAt(offset: number): { number: number; from: number; to: number };
	line(number: number): { from: number; to: number };
	sliceString(from: number, to: number): string;
}

/**
 * The whole lines `ranges` cover, merged where they touch: a range that
 * ends at the very start of a line does not take that line. Empty ranges
 * (cursors) are left out.
 */
export function lineSelections(
	doc: LineSource,
	ranges: readonly { from: number; to: number }[],
): LineSelection[] {
	const spans = ranges
		.filter((range) => range.to > range.from)
		.map((range) => {
			const start = doc.lineAt(range.from);
			let end = doc.lineAt(range.to);
			if (range.to === end.from && end.number > start.number) end = doc.lineAt(range.to - 1);
			return { start: start.number, end: end.number };
		})
		.toSorted((a, b) => a.start - b.start);
	const merged: { start: number; end: number }[] = [];
	for (const span of spans) {
		const last = merged.at(-1);
		if (last && span.start <= last.end + 1) last.end = Math.max(last.end, span.end);
		else merged.push({ ...span });
	}
	return merged.map(({ start, end }) => ({
		startLine: start,
		endLine: end,
		text: doc.sliceString(doc.line(start).from, doc.line(end).to),
	}));
}

/** `src/join.rs:40–58`, or `:40` for one line. */
export function linesLabel(path: string, startLine: number, endLine: number): string {
	return startLine === endLine ? `${path}:${startLine}` : `${path}:${startLine}–${endLine}`;
}

/**
 * Folding by indentation, where no syntax tree folds: the
 * lines after `line` that are indented deeper than it, blank lines inside
 * included, looking at most `limit` lines ahead; null when none are.
 * `text(n)` gives line `n` (from 1) of `count`.
 */
export function indentFold(
	text: (line: number) => string,
	count: number,
	line: number,
	tabSize: number,
	limit = 5000,
): { endLine: number } | null {
	const width = (content: string) => {
		let columns = 0;
		for (const char of content) {
			if (char === " ") columns += 1;
			else if (char === "\t") columns += tabSize - (columns % tabSize);
			else return columns;
		}
		return -1;
	};
	const own = width(text(line));
	if (own < 0) return null;
	let end = line;
	const last = Math.min(count, line + limit);
	for (let next = line + 1; next <= last; next++) {
		const depth = width(text(next));
		if (depth < 0) continue;
		if (depth <= own) break;
		end = next;
	}
	return end > line ? { endLine: end } : null;
}

// ---------------------------------------------------------------------------
// The status bar's facts about a file's text: line endings, indentation,
// encoding, and where the cursor is.

/** How lines end: LF (`\n`) or CRLF (`\r\n`). */
export type LineEnding = "LF" | "CRLF";

/** The separator a line ending writes. */
export function separatorOf(ending: LineEnding): string {
	return ending === "CRLF" ? "\r\n" : "\n";
}

/**
 * A text's line endings: the more common of CRLF and LF (LF on a tie or
 * with no line breaks); `mixed` when it has both. Lone CRs count as
 * neither.
 */
export function detectLineEnding(text: string): { ending: LineEnding; mixed: boolean } {
	let crlf = 0;
	let lf = 0;
	for (let at = text.indexOf("\n"); at !== -1; at = text.indexOf("\n", at + 1)) {
		if (at > 0 && text.charCodeAt(at - 1) === 13) crlf += 1;
		else lf += 1;
	}
	return { ending: crlf > lf ? "CRLF" : "LF", mixed: crlf > 0 && lf > 0 };
}

/** How a text indents: tabs, or spaces in steps of `size`. */
export interface Indentation {
	useTabs: boolean;
	size: number;
}

/**
 * How a text indents, as VS Code finds it: from the changes in leading
 * whitespace between consecutive lines. Tabs win when more lines start with
 * a tab than with spaces; else the most common step of spaces (2, 4 or 8,
 * or another from 1 to 8). Null when the text says too little (fewer than
 * two indented lines).
 */
export function detectIndentation(text: string, maxLines = 10_000): Indentation | null {
	let tabs = 0;
	let spaces = 0;
	const steps = new Map<number, number>();
	let previous = 0;
	let lines = 0;
	let start = 0;
	while (start <= text.length && lines < maxLines) {
		let end = text.indexOf("\n", start);
		if (end === -1) end = text.length;
		const line = text.slice(start, end).replace(/\r$/, "");
		start = end + 1;
		lines += 1;
		if (!line.trim()) continue;
		if (line.startsWith("\t")) {
			tabs += 1;
			previous = 0;
			continue;
		}
		let width = 0;
		while (line.charCodeAt(width) === 32) width += 1;
		if (width > 0) spaces += 1;
		const step = Math.abs(width - previous);
		// A lone space before a `*` is a block comment's, not indentation.
		if (step > 0 && step <= 8 && !(step === 1 && line[width] === "*"))
			steps.set(step, (steps.get(step) ?? 0) + 1);
		previous = width;
	}
	if (tabs + spaces < 2) return null;
	if (tabs > spaces) return { useTabs: true, size: 0 };
	let best = 0;
	let bestCount = 0;
	for (const [step, count] of steps) {
		// Ties go to the larger common step (4 over 2), as people mostly mean.
		if (count > bestCount || (count === bestCount && step > best)) {
			best = step;
			bestCount = count;
		}
	}
	return best ? { useTabs: false, size: best } : null;
}

/** The status bar's words for indentation: `Spaces: 2`, `Tab Size: 4`. */
export function indentationLabel(useTabs: boolean, tabSize: number): string {
	return t(useTabs ? "editor.tabSize" : "editor.spaces", { size: tabSize });
}

/** A file's encoding, from its text as the daemon sent it (always UTF-8). */
export function encodingOf(text: string): "UTF-8" | "UTF-8 with BOM" {
	return text.charCodeAt(0) === 0xfeff ? "UTF-8 with BOM" : "UTF-8";
}

/** The column a cursor shows at, from 1: tabs count to the next tab stop, as VS Code counts. */
export function visualColumn(lineText: string, offset: number, tabSize: number): number {
	let column = 0;
	for (let index = 0; index < offset && index < lineText.length; index++)
		column = lineText[index] === "\t" ? column + tabSize - (column % tabSize) : column + 1;
	return column + 1;
}

/** Where the cursor is, and what is selected, for the status bar. */
export interface CursorInfo {
	/** From 1. */
	line: number;
	/** From 1, tabs expanded. */
	column: number;
	/** Characters selected, across every selection. */
	selected: number;
	/** How many cursors or selections. */
	cursors: number;
}

/** `Ln 10, Col 1`, with `(3 selected)` or `(2 selections)` when there are. */
export function cursorLabel(cursor: CursorInfo): string {
	const at = { line: cursor.line, column: cursor.column };
	if (cursor.cursors > 1) return t("editor.selections", { ...at, count: cursor.cursors });
	return cursor.selected
		? t("editor.selected", { ...at, count: cursor.selected })
		: t("editor.cursor", at);
}
