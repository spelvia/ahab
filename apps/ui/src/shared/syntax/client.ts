/**
 * The page's side of the syntax engine: the known languages
 * (bundled, then plugins'), the worker, and what the rest of Ahab asks of
 * them. Loaded with the first thing to highlight (`load.ts`), never with
 * the app itself.
 *
 * Besides highlighting, it is the seam language tooling builds its approximate
 * navigation on: `tags`, `definitions`, `references` and `nameAt` read a
 * document's Tree-sitter tags (see the Editor documentation).
 */

import type {
	DocumentChange,
	DocumentSymbol,
	EditorDocument,
	EmitFolds,
	EmitTokens,
	HighlightSession,
	Highlighter,
	HighlightToken,
	TextRange,
} from "@/shared/editor/model";
import { patternLanguages } from "@/shared/preferences/store";
import { bundledLanguages } from "@/shared/syntax/bundled";
import { contributedLanguages, subscribeLanguages } from "@/shared/syntax/catalog";
import {
	detectLanguage,
	type Language,
	languageNamed,
	patternOf,
	PLAIN_TEXT,
	type Tag,
	tooLarge,
} from "@/shared/syntax/model";
import type { Calls, FromWorker, Method, Packed, Source, ToWorker } from "@/shared/syntax/protocol";

// --- Languages -------------------------------------------------------------------

let all: Language[] = [];
const listeners = new Set<() => void>();

function refresh() {
	const contributed = contributedLanguages();
	// A contributed language replaces a bundled one of the same ID.
	const replaced = new Set(contributed.map((item) => item.id));
	all = [...bundledLanguages.filter((item) => !replaced.has(item.id)), ...contributed];
	worker?.post({ type: "languages", languages: all });
	cache.clear();
	cachedBytes = 0;
	for (const listener of listeners) listener();
}

/** Every language, bundled first; later ones win where file patterns overlap. */
export function languages(): Language[] {
	return all;
}

/** Calls `listener` when plugins add or remove languages. */
export function onLanguagesChanged(listener: () => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

export function language(id: string): Language | undefined {
	return all.findLast((item) => item.id === id);
}

/** A language as a code fence or a person names it: `rust`, `rs`, `Rust`. */
export function named(name: string): Language | undefined {
	return languageNamed(all.toReversed(), name);
}

/**
 * A file's language: one chosen in this browser for its pattern (`*.h`),
 * else found by extension, name, `#!` or modeline. Undefined: plain text.
 */
export function detect(path: string, text?: string): Language | undefined {
	const chosen = patternLanguages()[patternOf(path)];
	if (chosen) return chosen === PLAIN_TEXT ? undefined : language(chosen);
	return detectLanguage(all, path, text);
}

// --- The worker ------------------------------------------------------------------------

interface Handlers {
	emit: EmitTokens;
	folds?: EmitFolds;
}

class Connection {
	private readonly worker = new Worker(new URL("./worker.ts", import.meta.url), {
		type: "module",
		name: "syntax",
	});
	private nextCall = 1;
	private readonly calls = new Map<
		number,
		{ resolve: (value: unknown) => void; reject: (error: Error) => void }
	>();
	readonly documents = new Map<number, Handlers>();
	private idle: ReturnType<typeof setTimeout> | undefined;

	constructor() {
		this.worker.addEventListener("message", (event: MessageEvent<FromWorker>) =>
			this.receive(event.data),
		);
		this.post({ type: "languages", languages: all });
	}

	post(message: ToWorker) {
		this.worker.postMessage(message, []);
		this.scheduleIdle();
	}

	private scheduleIdle() {
		clearTimeout(this.idle);
		this.idle = undefined;
		if (this.documents.size || this.calls.size) return;
		this.idle = setTimeout(() => {
			if (worker !== this) return;
			this.worker.terminate();
			worker = undefined;
		}, 60_000);
	}

	call<M extends Method>(method: M, args: Calls[M]["args"]): Promise<Calls[M]["result"]> {
		const id = this.nextCall++;
		return new Promise((resolve, reject) => {
			this.calls.set(id, { resolve: resolve as (value: unknown) => void, reject });
			this.post({ type: "call", id, method, args });
		});
	}

	private receive(message: FromWorker) {
		switch (message.type) {
			case "tokens":
				this.documents
					.get(message.doc)
					?.emit(unpack(message.tokens), { version: message.version, range: message.range });
				return;
			case "folds":
				this.documents.get(message.doc)?.folds?.(message.folds, { version: message.version });
				return;
			case "reply": {
				const waiting = this.calls.get(message.id);
				this.calls.delete(message.id);
				this.scheduleIdle();
				if (message.error !== undefined) waiting?.reject(new Error(message.error));
				else waiting?.resolve(message.result);
			}
		}
	}
}

let worker: Connection | undefined;
const connection = () => (worker ??= new Connection());
/** Without workers (tests in jsdom), nothing is highlighted. */
const workers = typeof Worker !== "undefined";

function unpack(packed: Packed): HighlightToken[] {
	const { data } = packed;
	const tokens: HighlightToken[] = Array.from({ length: data.length / 3 });
	for (let index = 0; index < tokens.length; index++)
		tokens[index] = {
			from: data[index * 3] ?? 0,
			to: data[index * 3 + 1] ?? 0,
			tag: packed.tags[data[index * 3 + 2] ?? 0] ?? "",
		};
	return tokens;
}

// --- Highlighting -----------------------------------------------------------------------

let nextDocument = 1;

/** The engine for editors: one worker document per open editor. */
export const highlighter: Highlighter = {
	open(document: EditorDocument, emit: EmitTokens, folds?: EmitFolds): HighlightSession | null {
		const text = document.text();
		const found =
			document.languageId !== undefined
				? document.languageId === PLAIN_TEXT
					? undefined
					: language(document.languageId)
				: detect(document.path, text);
		if (!workers || !found?.wasm || tooLarge(text)) return null;
		const doc = nextDocument++;
		const link = connection();
		link.documents.set(doc, { emit, folds });
		link.post({ type: "open", doc, language: found.id, text, version: document.version });
		return {
			update(change: DocumentChange) {
				link.post({ type: "update", doc, version: change.version, edits: change.edits });
			},
			dispose() {
				link.documents.delete(doc);
				link.post({ type: "close", doc });
			},
			symbols: (): Promise<DocumentSymbol[]> => link.call("symbols", { doc }),
			selectionRanges: (from: number, to: number): Promise<TextRange[]> =>
				link.call("selection", { doc, from, to }),
			comments: () => found.declaration.comments,
		};
	},
};

/** Highlighted texts kept, as code blocks show the same text again on every render. */
const cache = new Map<string, { tokens: Promise<HighlightToken[] | null>; bytes: number }>();
const CACHED_BYTES = 8 * 1024 * 1024;
let cachedBytes = 0;

function trimHighlights() {
	while (cachedBytes > CACHED_BYTES || cache.size > 200) {
		const oldest = cache.entries().next().value;
		if (!oldest) break;
		const [key, value] = oldest;
		cache.delete(key);
		cachedBytes -= value.bytes;
	}
}

/**
 * Highlights a text once, in a language given by ID or name (a code
 * fence's `ts`), or found from a path: null for plain text.
 */
export function highlight(
	text: string,
	as: { language: string } | { path: string },
): Promise<HighlightToken[] | null> {
	const found =
		"language" in as ? (language(as.language) ?? named(as.language)) : detect(as.path, text);
	if (!workers || !found?.wasm || tooLarge(text)) return Promise.resolve(null);
	const key = `${found.id}\u0000${text}`;
	const known = cache.get(key);
	if (known) {
		cache.delete(key);
		cache.set(key, known);
		return known.tokens;
	}
	const tokens = connection()
		.call("highlight", { text, language: found.id })
		.then(unpack)
		.catch(() => null);
	const entry = { tokens, bytes: key.length * 2 + 128 };
	cache.set(key, entry);
	cachedBytes += entry.bytes;
	trimHighlights();
	void tokens.then((result) => {
		if (cache.get(key) !== entry) return;
		// Approximate the retained token objects, whose tags are shared strings.
		const bytes = (result?.length ?? 0) * 56;
		entry.bytes += bytes;
		cachedBytes += bytes;
		trimHighlights();
	});
	return tokens;
}

/** Why a language did not load (a plugin's broken grammar or query), if it did not. */
export function problem(id: string): Promise<string | null> {
	if (!workers) return Promise.resolve(null);
	return connection().call("problem", { language: id });
}

// --- The seam: names defined and used ---------------------------------------------

/**
 * A text to read tags from: a text and its language (by ID), parsed once.
 * The approximate navigation reads open documents with
 * `{ text: document.text(), language }` and other files as search finds them.
 */
export type TagSource = Extract<Source, { text: string }>;

/** Every name the language's tags query finds: definitions and references, with lines. */
export function tags(source: TagSource): Promise<Tag[]> {
	if (!workers || !language(source.language)?.queries.tags) return Promise.resolve([]);
	return connection().call("tags", source);
}

/** Where `name` is defined in a text (functions, classes, …), as tags say. */
export async function definitions(name: string, source: TagSource): Promise<Tag[]> {
	return (await tags(source)).filter((tag) => tag.role === "definition" && tag.name === name);
}

/** Where `name` is used in a text, as tags say (calls, type uses, …). */
export async function references(name: string, source: TagSource): Promise<Tag[]> {
	return (await tags(source)).filter((tag) => tag.role === "reference" && tag.name === name);
}

/** The identifier at `offset` of a text: what to look for. */
export function nameAt(
	source: TagSource,
	offset: number,
): Promise<{ name: string; from: number; to: number } | null> {
	if (!workers || !language(source.language)?.wasm) return Promise.resolve(null);
	return connection().call("nameAt", { ...source, offset });
}

refresh();
subscribeLanguages(refresh);
