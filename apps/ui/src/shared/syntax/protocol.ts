/**
 * The messages between the syntax worker (`worker.ts`) and the page
 * (`client.ts`). Tokens travel packed: three numbers each (from, to, the
 * index of its tag in `tags`).
 */

import type { DocumentSymbol, TextEdit, TextRange } from "@/shared/editor/model";
import type { Language, Tag } from "@/shared/syntax/model";

/** A text to work on once: an open document, or a text and its language. */
export type Source = { doc: number } | { text: string; language: string };

export interface Calls {
	/** Highlights a text once (a code block, a side of a diff). */
	highlight: { args: { text: string; language: string }; result: Packed };
	symbols: { args: { doc: number }; result: DocumentSymbol[] };
	selection: { args: { doc: number; from: number; to: number }; result: TextRange[] };
	tags: { args: Source; result: Tag[] };
	nameAt: {
		args: Source & { offset: number };
		result: { name: string; from: number; to: number } | null;
	};
	/** Why a language did not load, if it did not. */
	problem: { args: { language: string }; result: string | null };
}

export type Method = keyof Calls;

export interface Packed {
	tags: string[];
	data: Uint32Array;
}

export type ToWorker =
	| { type: "languages"; languages: Language[] }
	| { type: "open"; doc: number; language: string; text: string; version: number }
	| { type: "update"; doc: number; version: number; edits: TextEdit[] }
	| { type: "close"; doc: number }
	| { type: "call"; id: number; method: Method; args: Calls[Method]["args"] };

export type FromWorker =
	| { type: "tokens"; doc: number; version: number; range?: TextRange; tokens: Packed }
	| { type: "folds"; doc: number; version: number; folds: TextRange[] }
	| { type: "reply"; id: number; result?: unknown; error?: string };
