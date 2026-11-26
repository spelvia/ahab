/**
 * The syntax engine's page side, loaded the first time something is
 * highlighted, so the app's first load carries nothing of it.
 */

import { useEffect, useState } from "react";

import type {
	EditorDocument,
	EmitFolds,
	EmitTokens,
	HighlightSession,
	Highlighter,
} from "@/shared/editor/model";

export type Syntax = typeof import("@/shared/syntax/client");

let loading: Promise<Syntax> | undefined;
let loaded: Syntax | undefined;

export function loadSyntax(): Promise<Syntax> {
	loading ??= import("@/shared/syntax/client").then((module) => {
		loaded = module;
		return module;
	});
	return loading;
}

/** The syntax engine, once loaded (it starts loading on first use). */
export function useSyntax(): Syntax | undefined {
	const [syntax, setSyntax] = useState(loaded);
	useEffect(() => {
		if (syntax) return;
		let live = true;
		void loadSyntax().then((module) => {
			if (live) setSyntax(module);
		});
		return () => {
			live = false;
		};
	}, [syntax]);
	return syntax;
}

/**
 * The Tree-sitter engine as the editor's highlighter: answers at once and
 * starts highlighting when the engine has loaded. The document's text and
 * version are read then, so edits made meanwhile are not lost.
 */
export const syntaxHighlighter: Highlighter = {
	open(document: EditorDocument, emit: EmitTokens, folds?: EmitFolds): HighlightSession {
		let inner: HighlightSession | null = null;
		let disposed = false;
		const started = loadSyntax().then((module) => {
			if (!disposed) inner = module.highlighter.open(document, emit, folds);
		});
		return {
			update: (change) => inner?.update(change),
			dispose: () => {
				disposed = true;
				inner?.dispose();
			},
			symbols: async () => {
				await started;
				return (await inner?.symbols?.()) ?? [];
			},
			selectionRanges: async (from, to) => {
				await started;
				return (await inner?.selectionRanges?.(from, to)) ?? [];
			},
			comments: () => inner?.comments?.(),
		};
	},
};
