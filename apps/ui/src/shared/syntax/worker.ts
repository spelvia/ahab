/**
 * The syntax worker: parsing and highlighting off the page's
 * thread, so large files never block typing or scrolling. One worker
 * serves every editor, diff and code block of the page.
 */

import runtime from "web-tree-sitter/web-tree-sitter.wasm?url";

import type { HighlightToken } from "@/shared/editor/model";
import {
	gzipped,
	initParser,
	type Loaded,
	SyntaxDocument,
	SyntaxEngine,
} from "@/shared/syntax/engine";
import type { Calls, FromWorker, Method, Packed, Source, ToWorker } from "@/shared/syntax/protocol";

/** Highlighting goes in pieces this large, so the first lines show soon. */
const CHUNK = 64 * 1024;
/** Folds wait for typing to pause. */
const FOLD_DELAY = 300;

const scope = self as unknown as {
	postMessage(message: FromWorker, transfer: Transferable[]): void;
	addEventListener(type: "message", listener: (event: MessageEvent<ToWorker>) => void): void;
};

function send(message: FromWorker, transfer: Transferable[] = []) {
	scope.postMessage(message, transfer);
}

async function unpack(bytes: Uint8Array): Promise<Uint8Array> {
	if (!gzipped(bytes)) return bytes;
	const stream = new Blob([bytes as BlobPart])
		.stream()
		.pipeThrough(new DecompressionStream("gzip"));
	return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function get(url: string): Promise<Response> {
	const answer = await fetch(url);
	if (!answer.ok) throw new Error(`${url}: ${answer.status} ${answer.statusText}`);
	return answer;
}

const engine = new SyntaxEngine({
	bytes: async (url) => unpack(new Uint8Array(await (await get(url)).arrayBuffer())),
	text: async (url) => (await get(url)).text(),
});
void initParser(runtime);

function pack(tokens: HighlightToken[]): { packed: Packed; transfer: Transferable[] } {
	const tags: string[] = [];
	const ids = new Map<string, number>();
	const data = new Uint32Array(tokens.length * 3);
	tokens.forEach((token, index) => {
		let id = ids.get(token.tag);
		if (id === undefined) {
			id = tags.length;
			tags.push(token.tag);
			ids.set(token.tag, id);
		}
		data[index * 3] = token.from;
		data[index * 3 + 1] = token.to;
		data[index * 3 + 2] = id;
	});
	return { packed: { tags, data }, transfer: [data.buffer] };
}

// --- Open documents -------------------------------------------------------------

interface Open {
	/** Each message for the document runs after the one before. */
	queue: Promise<void>;
	document?: SyntaxDocument;
	closed: boolean;
	foldTimer?: ReturnType<typeof setTimeout>;
}

const documents = new Map<number, Open>();

function enqueue(doc: number, work: (open: Open) => Promise<void> | void) {
	const open = documents.get(doc);
	if (!open) return;
	open.queue = open.queue.then(async () => {
		if (open.closed) return;
		try {
			await work(open);
		} catch {
			// One failed step leaves the document as it was; the next tries again.
		}
	});
}

/** Highlights `from`–`to` of a document in pieces, each sent as soon as it is ready. */
async function highlightRange(doc: number, open: Open, from: number, to: number) {
	const document = open.document;
	if (!document) return;
	const { text, version } = document;
	for (let start = from; start < to;) {
		const tree = document.current;
		if (open.closed || !tree || document.version !== version) return;
		let end = Math.min(to, start + CHUNK);
		if (end < to) {
			const lineEnd = text.indexOf("\n", end);
			end = lineEnd === -1 || lineEnd > to ? to : lineEnd;
		}
		// One piece after the other, each sent as soon as it is ready.
		// oxlint-disable-next-line no-await-in-loop
		const tokens = await engine.highlight(document.loaded, tree, text, start, end);
		const { packed, transfer } = pack(tokens);
		send(
			{ type: "tokens", doc, version, range: { from: start, to: end }, tokens: packed },
			transfer,
		);
		start = end;
	}
}

function scheduleFolds(doc: number, open: Open, delay: number) {
	clearTimeout(open.foldTimer);
	open.foldTimer = setTimeout(() => {
		enqueue(doc, ({ document }) => {
			const tree = document?.current;
			if (!document || !tree) return;
			const folds = engine.folds(document.loaded, tree, document.text);
			send({ type: "folds", doc, version: document.version, folds });
		});
	}, delay);
}

async function loaded(id: string): Promise<Loaded> {
	const language = engine.language(id);
	if (!language) throw new Error(`No language ${id}.`);
	const found = await engine.load(language);
	if (!found) throw new Error(engine.problems.get(id) ?? `${language.name} has no grammar.`);
	return found;
}

/** Runs `work` on a source's tree: an open document's, or a text parsed once. */
async function withTree<T>(
	source: Source,
	work: (language: Loaded, tree: NonNullable<SyntaxDocument["current"]>, text: string) => T,
): Promise<T | null> {
	if ("doc" in source) {
		const open = documents.get(source.doc);
		if (!open) return null;
		// After the messages before it, so it sees the version last sent.
		await open.queue;
		const document = open.document;
		const tree = document?.current;
		return document && tree ? work(document.loaded, tree, document.text) : null;
	}
	const language = await loaded(source.language);
	const tree = engine.parse(language, source.text);
	if (!tree) return null;
	try {
		return work(language, tree, source.text);
	} finally {
		tree.delete();
	}
}

// --- Calls -----------------------------------------------------------------------------

const calls: { [M in Method]: (args: Calls[M]["args"]) => Promise<Calls[M]["result"]> } = {
	highlight: async ({ text, language }) => {
		const tokens = await engine.highlightText(await loaded(language), text);
		return pack(tokens).packed;
	},
	symbols: async (source) =>
		(await withTree(source, (language, tree) => engine.symbols(language, tree))) ?? [],
	selection: async ({ doc, from, to }) =>
		(await withTree({ doc }, (_language, tree) => engine.selectionRanges(tree, from, to))) ?? [],
	tags: async (source) =>
		(await withTree(source, (language, tree) => engine.tags(language, tree))) ?? [],
	nameAt: async (source) =>
		(await withTree(source, (_language, tree) => engine.nameAt(tree, source.offset))) ?? null,
	problem: async ({ language }) => engine.problems.get(language) ?? null,
};

scope.addEventListener("message", (event) => {
	const message = event.data;
	switch (message.type) {
		case "languages":
			engine.setLanguages(message.languages);
			return;
		case "open": {
			documents.set(message.doc, { queue: Promise.resolve(), closed: false });
			enqueue(message.doc, async (open) => {
				const language = await loaded(message.language).catch(() => null);
				if (!language || open.closed) return;
				open.document = new SyntaxDocument(engine, language, message.text, message.version);
				await highlightRange(message.doc, open, 0, message.text.length);
				scheduleFolds(message.doc, open, 0);
			});
			return;
		}
		case "update":
			enqueue(message.doc, async (open) => {
				const document = open.document;
				if (!document) return;
				const ranges = document.update(message.edits, message.version);
				for (const range of ranges)
					// In order: the ranges share the document's one tree.
					// oxlint-disable-next-line no-await-in-loop
					await highlightRange(message.doc, open, range.from, range.to);
				scheduleFolds(message.doc, open, FOLD_DELAY);
			});
			return;
		case "close": {
			const open = documents.get(message.doc);
			if (!open) return;
			open.closed = true;
			clearTimeout(open.foldTimer);
			documents.delete(message.doc);
			void open.queue.then(() => open.document?.dispose());
			return;
		}
		case "call": {
			const run = calls[message.method] as (args: unknown) => Promise<unknown>;
			run(message.args).then(
				(result) => send({ type: "reply", id: message.id, result }),
				(error: unknown) =>
					send({
						type: "reply",
						id: message.id,
						error: error instanceof Error ? error.message : String(error),
					}),
			);
		}
	}
});
