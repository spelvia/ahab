/**
 * The syntax engine: Tree-sitter through web-tree-sitter.
 * Grammars and queries load the first time a language is needed; documents
 * parse incrementally as they change; highlighting follows the queries'
 * captures, with injections (code in Markdown fences, CSS and JavaScript in
 * HTML) highlighted by their own languages. It also finds folds, symbols
 * (the outline), tags and the syntax
 * nodes around a selection.
 *
 * It runs in the worker (`worker.ts`) and, in tests, in Node; it knows
 * nothing of either: `Loader` fetches the files.
 */

import {
	Edit,
	Language as Grammar,
	type Node,
	Parser,
	type Point,
	Query,
	type QueryMatch,
	type Range,
	type Tree,
} from "web-tree-sitter";

import type { DocumentSymbol, HighlightToken, TextEdit } from "@/shared/editor/model";
import {
	detectLanguage,
	type Language,
	languageNamed,
	nestSymbols,
	QUERY_KINDS,
	type QueryKind,
	type Tag,
} from "@/shared/syntax/model";

/** Fetches a grammar (unpacked when gzipped) and a query's text. */
export interface Loader {
	bytes(url: string): Promise<Uint8Array>;
	text(url: string): Promise<string>;
}

/** A language with its grammar and queries loaded. */
export interface Loaded {
	language: Language;
	grammar: Grammar;
	queries: Partial<Record<QueryKind, Query>>;
}

/** A fold: from the end of its first line to `to`. */
export interface Fold {
	from: number;
	to: number;
}

/** How deep injections nest (Markdown → HTML → JavaScript). */
const MAX_DEPTH = 3;

const IDENTIFIER =
	/identifier|^name$|^word$|^tag_name$|^property|^field|^constant$|^key$|^atom$|^symbol$/;

/** Whether a fetched grammar is gzipped (`1f 8b`), rather than WebAssembly (`00 61 73 6d`). */
export function gzipped(bytes: Uint8Array): boolean {
	return bytes[0] === 0x1f && bytes[1] === 0x8b;
}

let ready: Promise<void> | undefined;

/** Starts web-tree-sitter once; `runtime` is the address of its own WebAssembly. */
export function initParser(runtime?: string): Promise<void> {
	ready ??= Parser.init(runtime ? { locateFile: () => runtime } : undefined);
	return ready;
}

/** The point (row, column in UTF-16 units) of `index` in `text`. */
function pointAt(text: string, index: number): Point {
	let row = 0;
	let lineStart = 0;
	for (let at = text.indexOf("\n"); at !== -1 && at < index; at = text.indexOf("\n", at + 1)) {
		row++;
		lineStart = at + 1;
	}
	return { row, column: index - lineStart };
}

/** Points of several indices of the same text, in one pass. */
function pointsAt(text: string, indices: number[]): Map<number, Point> {
	const sorted = [...new Set(indices)].toSorted((a, b) => a - b);
	const found = new Map<number, Point>();
	let row = 0;
	let lineStart = 0;
	let at = text.indexOf("\n");
	for (const index of sorted) {
		while (at !== -1 && at < index) {
			row++;
			lineStart = at + 1;
			at = text.indexOf("\n", at + 1);
		}
		found.set(index, { row, column: index - lineStart });
	}
	return found;
}

// --- Predicates web-tree-sitter leaves to us ---------------------------------------

/** Whether a match passes the predicates Helix's queries use beyond Tree-sitter's own. */
function passes(query: Query, match: QueryMatch): boolean {
	const predicates = query.predicates[match.patternIndex];
	if (!predicates?.length) return true;
	const nodes = (name: string) =>
		match.captures.filter((capture) => capture.name === name).map((capture) => capture.node);
	for (const { operator, operands } of predicates) {
		if (operator.endsWith("!")) continue;
		const captures = operands
			.filter((operand) => operand.type === "capture")
			.map((operand) => operand.name);
		const strings = new Set(
			operands.filter((operand) => operand.type === "string").map((operand) => operand.value),
		);
		const first = nodes(captures[0] ?? "");
		const negated = operator.startsWith("not-");
		const name = negated ? operator.slice(4) : operator;
		let result: boolean;
		switch (name) {
			case "kind-eq?":
				result = first.every((node) => strings.has(node.type));
				break;
			case "same-line?": {
				const second = nodes(captures[1] ?? "");
				result = first.every((a) =>
					second.every((b) => a.startPosition.row === b.startPosition.row),
				);
				break;
			}
			case "one-line?":
				result = first.every((node) => node.startPosition.row === node.endPosition.row);
				break;
			case "has-ancestor?":
			case "has-parent?": {
				const parentOnly = name === "has-parent?";
				result = first.every((node) => {
					for (let up = node.parent; up; up = parentOnly ? null : up.parent)
						if (strings.has(up.type)) return true;
					return false;
				});
				break;
			}
			default:
				// Anything else (`is-not? local`, Helix's own): not ours to judge.
				continue;
		}
		if (result === negated) return false;
	}
	return true;
}

function matchesOf(query: Query, node: Node, from?: number, to?: number): QueryMatch[] {
	try {
		const options = from === undefined ? undefined : { startIndex: from, endIndex: to };
		return query.matches(node, options).filter((match) => passes(query, match));
	} catch {
		// A pattern Tree-sitter's JavaScript cannot evaluate (a regular
		// expression in Rust's syntax): no highlighting rather than an error.
		return [];
	}
}

// --- The engine ------------------------------------------------------------------------

interface Painted {
	from: number;
	to: number;
	tag: string;
	layer: number;
	pattern: number;
}

/** A language's files, as a key: a changed declaration loads again. */
function keyOf(language: Language): string {
	return JSON.stringify([language.wasm, QUERY_KINDS.map((kind) => language.queries[kind])]);
}

export class SyntaxEngine {
	private languages: Language[] = [];
	private readonly loading = new Map<string, { key: string; loaded: Promise<Loaded | null> }>();
	/** Made once web-tree-sitter has started, before the first language loads. */
	private parser: Parser | undefined;
	/** Problems loading a language, by ID, for the person to see. */
	readonly problems = new Map<string, string>();

	constructor(private readonly loader: Loader) {}

	/** The languages known, later ones winning (plugins after the bundled ones). */
	setLanguages(languages: Language[]) {
		this.languages = languages;
	}

	language(id: string): Language | undefined {
		return this.languages.findLast((language) => language.id === id);
	}

	detect(path: string, text?: string): Language | undefined {
		return detectLanguage(this.languages, path, text);
	}

	named(name: string): Language | undefined {
		return languageNamed(this.languages.toReversed(), name);
	}

	/** A language's grammar and queries, loaded once; null without a grammar or when they fail. */
	load(language: Language): Promise<Loaded | null> {
		const key = keyOf(language);
		const known = this.loading.get(language.id);
		if (known?.key === key) return known.loaded;
		const loaded = this.fetch(language);
		this.loading.set(language.id, { key, loaded });
		return loaded;
	}

	private async fetch(language: Language): Promise<Loaded | null> {
		if (!language.wasm) return null;
		try {
			await initParser();
			this.parser ??= new Parser();
			const grammar = await Grammar.load(await this.loader.bytes(language.wasm));
			const queries: Partial<Record<QueryKind, Query>> = {};
			const failed: string[] = [];
			await Promise.all(
				QUERY_KINDS.map(async (kind) => {
					const url = language.queries[kind];
					if (!url) return;
					try {
						queries[kind] = new Query(grammar, await this.loader.text(url));
					} catch (error) {
						failed.push(`${kind}: ${error instanceof Error ? error.message : String(error)}`);
					}
				}),
			);
			if (failed.length) this.problems.set(language.id, failed.join("; "));
			else this.problems.delete(language.id);
			return { language, grammar, queries };
		} catch (error) {
			this.problems.set(language.id, error instanceof Error ? error.message : String(error));
			return null;
		}
	}

	/** Parses `text`: once, or again from an edited `old` tree. */
	parse(loaded: Loaded, text: string, ranges?: Range[], old?: Tree | null): Tree | null {
		if (!this.parser) return null;
		this.parser.setLanguage(loaded.grammar);
		return this.parser.parse(text, old ?? null, ranges ? { includedRanges: ranges } : undefined);
	}

	/** Highlighted tokens of `text` between `from` and `to`, injections included. */
	async highlight(
		loaded: Loaded,
		tree: Tree,
		text: string,
		from = 0,
		to = text.length,
	): Promise<HighlightToken[]> {
		const painted = await this.paintable(loaded, tree, text, from, to, 0);
		return paint(painted, from, to);
	}

	/** Highlights a whole text at once (a code block, a diff's side). */
	async highlightText(loaded: Loaded, text: string): Promise<HighlightToken[]> {
		const tree = this.parse(loaded, text);
		if (!tree) return [];
		try {
			return await this.highlight(loaded, tree, text);
		} finally {
			tree.delete();
		}
	}

	private async paintable(
		loaded: Loaded,
		tree: Tree,
		text: string,
		from: number,
		to: number,
		layer: number,
	): Promise<Painted[]> {
		const found: Painted[] = [];
		const highlights = loaded.queries.highlights;
		if (highlights) {
			for (const match of matchesOf(highlights, tree.rootNode, from, to)) {
				for (const capture of match.captures) {
					const name = capture.name;
					if (name.startsWith("_") || name.startsWith("injection.") || name.startsWith("local."))
						continue;
					found.push({
						from: capture.node.startIndex,
						to: capture.node.endIndex,
						tag: name,
						layer,
						pattern: match.patternIndex,
					});
				}
			}
		}
		if (layer < MAX_DEPTH && loaded.queries.injections) {
			const injections = this.injections(loaded.queries.injections, tree, from, to);
			const inner = await Promise.all(
				injections.map(async (injection) => {
					const language = this.injected(injection.name, text, injection.ranges);
					const grammar = language ? await this.load(language) : null;
					if (!grammar) return [];
					const start = Math.max(from, injection.ranges[0]?.startIndex ?? from);
					const end = Math.min(to, injection.ranges.at(-1)?.endIndex ?? to);
					if (end <= start) return [];
					// Parsing is synchronous: the one parser is never shared across an await.
					const innerTree = this.parse(grammar, text, injection.ranges);
					if (!innerTree) return [];
					try {
						return await this.paintable(grammar, innerTree, text, start, end, layer + 1);
					} finally {
						innerTree.delete();
					}
				}),
			);
			for (const list of inner) found.push(...list);
		}
		return found;
	}

	/** The language an injection names: by name, by `#!`, or by a file name. */
	private injected(name: InjectionName, text: string, ranges: Range[]): Language | undefined {
		if (name.kind === "language") return this.named(name.value);
		if (name.kind === "filename") return this.detect(name.value);
		const start = ranges[0]?.startIndex ?? 0;
		const firstLine = text.slice(
			start,
			text.indexOf("\n", start) === -1 ? undefined : text.indexOf("\n", start),
		);
		return firstLine.startsWith("#!") ? this.detect("", firstLine) : undefined;
	}

	/** The injections between `from` and `to`: a language and the ranges it covers. */
	private injections(query: Query, tree: Tree, from: number, to: number) {
		const found: { name: InjectionName; ranges: Range[] }[] = [];
		const combined = new Map<string, { name: InjectionName; ranges: Range[] }>();
		for (const match of matchesOf(query, tree.rootNode, from, to)) {
			const properties = { ...query.setProperties[match.patternIndex], ...match.setProperties };
			let name: InjectionName | undefined;
			const content: Node[] = [];
			for (const capture of match.captures) {
				if (capture.name === "injection.content") content.push(capture.node);
				else if (capture.name === "injection.language")
					name = { kind: "language", value: capture.node.text };
				else if (capture.name === "injection.filename")
					name = { kind: "filename", value: capture.node.text };
				else if (capture.name === "injection.shebang" && !name)
					name = { kind: "shebang", value: "" };
			}
			const set = properties["injection.language"];
			if (typeof set === "string" && (!name || name.kind === "shebang"))
				name = { kind: "language", value: set };
			if (!name || !content.length) continue;
			const children =
				"injection.include-children" in properties
					? "all"
					: "injection.include-unnamed-children" in properties
						? "unnamed"
						: "none";
			const ranges = content.flatMap((node) => rangesOf(node, children));
			if (!ranges.length) continue;
			if ("injection.combined" in properties) {
				const key = `${match.patternIndex}:${name.kind}:${name.value}`;
				const group = combined.get(key);
				if (group) group.ranges.push(...ranges);
				else combined.set(key, { name, ranges });
			} else found.push({ name, ranges });
		}
		for (const group of combined.values()) {
			group.ranges.sort((a, b) => a.startIndex - b.startIndex);
			found.push(group);
		}
		return found;
	}

	// --- Structure --------------------------------------------------------------------------

	/** Where the document folds: the language's folds query, else its multi-line blocks. */
	folds(loaded: Loaded, tree: Tree, text: string): Fold[] {
		const nodes: Node[] = [];
		const query = loaded.queries.folds;
		if (query) {
			for (const match of matchesOf(query, tree.rootNode))
				for (const capture of match.captures) if (capture.name === "fold") nodes.push(capture.node);
		} else {
			// The outermost multi-line node starting on each line.
			const byRow = new Map<number, Node>();
			const visit = (node: Node) => {
				for (const child of node.namedChildren) {
					if (!child || child.endPosition.row <= child.startPosition.row) continue;
					const row = child.startPosition.row;
					const held = byRow.get(row);
					if (!held || held.endIndex < child.endIndex) byRow.set(row, child);
					visit(child);
				}
			};
			visit(tree.rootNode);
			nodes.push(...byRow.values());
		}
		const folds = new Map<number, Fold>();
		for (const node of nodes) {
			const start = node.startPosition.row;
			let end = node.endPosition.row;
			if (end <= start) continue;
			const lineEnd = text.indexOf("\n", node.startIndex);
			if (lineEnd === -1) continue;
			let to = node.endIndex;
			// A closing bracket on a line of its own stays visible: `{ … }`.
			const lastLineStart = text.lastIndexOf("\n", node.endIndex - 1) + 1;
			const before = text.slice(lastLineStart, node.endIndex);
			if (/^\s*[)\]}>]|^\s*(end|fi|done|esac)\b|^\s*<\//.test(before)) {
				to = lastLineStart - 1;
				end -= 1;
			}
			if (end <= start || to <= lineEnd) continue;
			const held = folds.get(lineEnd);
			if (!held || held.to < to) folds.set(lineEnd, { from: lineEnd, to });
		}
		return [...folds.values()].toSorted((a, b) => a.from - b.from);
	}

	/** Names defined and used, from the language's tags query. */
	tags(loaded: Loaded, tree: Tree): Tag[] {
		const query = loaded.queries.tags;
		if (!query) return [];
		const found: Tag[] = [];
		for (const match of matchesOf(query, tree.rootNode)) {
			const name = match.captures.find((capture) => capture.name === "name")?.node;
			const whole = match.captures.find(
				(capture) =>
					capture.name.startsWith("definition.") || capture.name.startsWith("reference."),
			);
			if (!name || !whole) continue;
			const [role, ...kind] = whole.name.split(".");
			let node = whole.node;
			// A Markdown heading's section holds what is under it.
			if (node.parent?.type === "section" && node.parent.firstNamedChild?.id === node.id)
				node = node.parent;
			found.push({
				name: name.text.trim(),
				role: role === "definition" ? "definition" : "reference",
				kind: kind.join(".") || "symbol",
				from: node.startIndex,
				to: node.endIndex,
				nameFrom: name.startIndex,
				nameTo: name.endIndex,
				line: name.startPosition.row + 1,
			});
		}
		return found;
	}

	/** The outline: definitions, nested by what contains what. */
	symbols(loaded: Loaded, tree: Tree): DocumentSymbol[] {
		const seen = new Set<string>();
		const flat = this.tags(loaded, tree)
			.filter((tag) => tag.role === "definition" && tag.name)
			.filter((tag) => {
				const key = `${tag.from}:${tag.to}:${tag.nameFrom}`;
				if (seen.has(key)) return false;
				seen.add(key);
				return true;
			})
			.map(({ name, kind, from, to, nameFrom, nameTo }) => ({
				name,
				kind,
				from,
				to,
				nameFrom,
				nameTo,
			}));
		return nestSymbols(flat);
	}

	/** The syntax nodes around `from`–`to`, innermost first, each larger than the last. */
	selectionRanges(tree: Tree, from: number, to: number): { from: number; to: number }[] {
		const ranges: { from: number; to: number }[] = [];
		let node: Node | null = tree.rootNode.descendantForIndex(from, Math.max(from, to));
		while (node) {
			const range = { from: node.startIndex, to: node.endIndex };
			const last = ranges.at(-1);
			if (
				range.from <= from &&
				range.to >= to &&
				(!last || range.from < last.from || range.to > last.to)
			)
				ranges.push(range);
			node = node.parent;
		}
		return ranges;
	}

	/** The identifier at `offset` (or just before it), as tags name things. */
	nameAt(tree: Tree, offset: number): { name: string; from: number; to: number } | null {
		for (const at of [offset, offset - 1]) {
			if (at < 0) continue;
			const node = tree.rootNode.descendantForIndex(at, at);
			if (
				node &&
				node.childCount === 0 &&
				node.isNamed &&
				IDENTIFIER.test(node.type) &&
				node.text.trim()
			)
				return { name: node.text, from: node.startIndex, to: node.endIndex };
		}
		return null;
	}
}

type InjectionName = { kind: "language" | "filename" | "shebang"; value: string };

/** A node's ranges for an injection: without its children, its named ones, or with all. */
function rangesOf(node: Node, children: "none" | "unnamed" | "all"): Range[] {
	const whole: Range = {
		startIndex: node.startIndex,
		endIndex: node.endIndex,
		startPosition: node.startPosition,
		endPosition: node.endPosition,
	};
	if (children === "all") return [whole];
	const ranges: Range[] = [];
	let cursor = { index: node.startIndex, point: node.startPosition };
	for (const child of node.children) {
		if (!child || (children === "unnamed" && !child.isNamed)) continue;
		if (child.startIndex > cursor.index)
			ranges.push({
				startIndex: cursor.index,
				endIndex: child.startIndex,
				startPosition: cursor.point,
				endPosition: child.startPosition,
			});
		cursor = { index: child.endIndex, point: child.endPosition };
	}
	if (node.endIndex > cursor.index)
		ranges.push({
			startIndex: cursor.index,
			endIndex: node.endIndex,
			startPosition: cursor.point,
			endPosition: node.endPosition,
		});
	return ranges;
}

/**
 * Captures painted in order, the later over the earlier: outer layers
 * before injected ones, larger nodes before the nodes inside them, and for
 * the same node, earlier patterns before later ones (in Helix's queries the
 * later pattern wins). The result: tokens that do not overlap.
 */
export function paint(items: Painted[], from: number, to: number): HighlightToken[] {
	const length = to - from;
	if (length <= 0 || !items.length) return [];
	const tags: string[] = [""];
	const ids = new Map<string, number>();
	const cells = new Uint16Array(length);
	const ordered = items.toSorted(
		(a, b) => a.layer - b.layer || b.to - b.from - (a.to - a.from) || a.pattern - b.pattern,
	);
	for (const item of ordered) {
		const start = Math.max(item.from, from) - from;
		const end = Math.min(item.to, to) - from;
		if (end <= start) continue;
		let id = ids.get(item.tag);
		if (id === undefined) {
			id = tags.length;
			tags.push(item.tag);
			ids.set(item.tag, id);
		}
		cells.fill(id, start, end);
	}
	const tokens: HighlightToken[] = [];
	let start = 0;
	for (let index = 1; index <= length; index++) {
		if (index < length && cells[index] === cells[start]) continue;
		const id = cells[start] ?? 0;
		if (id) tokens.push({ from: from + start, to: from + index, tag: tags[id] ?? "" });
		start = index;
	}
	return tokens;
}

/** A document kept parsed: the editor's text, version by version. */
export class SyntaxDocument {
	private tree: Tree | null;

	constructor(
		private readonly engine: SyntaxEngine,
		readonly loaded: Loaded,
		public text: string,
		public version: number,
	) {
		this.tree = engine.parse(loaded, text);
	}

	get current(): Tree | null {
		return this.tree;
	}

	/**
	 * Applies the edits that made `version` (each in the coordinates of the
	 * text before them all, as `DocumentChange` gives them) and parses again
	 * from the old tree. Returns the ranges whose highlighting may have
	 * changed, in the new text.
	 */
	update(edits: TextEdit[], version: number): { from: number; to: number }[] {
		const ordered = edits.toSorted((a, b) => b.from - a.from);
		let text = this.text;
		const old = this.tree;
		for (const edit of ordered) {
			const points = pointsAt(text, [edit.from, edit.to]);
			const start = points.get(edit.from) ?? pointAt(text, edit.from);
			const oldEnd = points.get(edit.to) ?? pointAt(text, edit.to);
			const lines = edit.insert.split("\n");
			const newEnd =
				lines.length === 1
					? { row: start.row, column: start.column + edit.insert.length }
					: { row: start.row + lines.length - 1, column: lines.at(-1)?.length ?? 0 };
			old?.edit(
				new Edit({
					startIndex: edit.from,
					oldEndIndex: edit.to,
					newEndIndex: edit.from + edit.insert.length,
					startPosition: start,
					oldEndPosition: oldEnd,
					newEndPosition: newEnd,
				}),
			);
			text = text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
		}
		this.text = text;
		this.version = version;
		const tree = this.engine.parse(this.loaded, text, undefined, old);
		const changed: { from: number; to: number }[] = [];
		if (old && tree)
			for (const range of old.getChangedRanges(tree))
				changed.push({ from: range.startIndex, to: range.endIndex });
		old?.delete();
		this.tree = tree;
		// The edited text itself, in the new coordinates.
		let shift = 0;
		for (const edit of edits.toSorted((a, b) => a.from - b.from)) {
			const from = edit.from + shift;
			changed.push({ from, to: from + edit.insert.length });
			shift += edit.insert.length - (edit.to - edit.from);
		}
		return mergeLines(text, changed);
	}

	dispose() {
		this.tree?.delete();
		this.tree = null;
	}
}

/** Ranges widened to whole lines and merged. */
export function mergeLines(
	text: string,
	ranges: { from: number; to: number }[],
): { from: number; to: number }[] {
	const widened = ranges
		.map(({ from, to }) => {
			const start = text.lastIndexOf("\n", Math.max(0, from - 1)) + 1;
			const next = text.indexOf("\n", to);
			return { from: from === 0 ? 0 : start, to: next === -1 ? text.length : next };
		})
		.toSorted((a, b) => a.from - b.from);
	const merged: { from: number; to: number }[] = [];
	for (const range of widened) {
		const last = merged.at(-1);
		if (last && range.from <= last.to + 1) last.to = Math.max(last.to, range.to);
		else merged.push({ ...range });
	}
	return merged;
}
