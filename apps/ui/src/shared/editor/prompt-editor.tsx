/**
 * A message box with Vim's keys: CodeMirror in place of a textarea, for a
 * composer when Vim is on (Settings → Editor). Enter sends from any mode;
 * Shift+Enter starts a new line. Vim's mode shows in the status bar, as a
 * file's editor's does.
 */

import { history, historyKeymap, defaultKeymap, insertNewline } from "@codemirror/commands";
import { Compartment, EditorState, Prec } from "@codemirror/state";
import {
	drawSelection,
	EditorView,
	keymap,
	placeholder as placeholderText,
} from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

export interface PromptEditorHandle {
	focus: () => void;
	replaceRange: (from: number, to: number) => void;
}

const theme = EditorView.theme({
	"&": { fontSize: "inherit", backgroundColor: "transparent" },
	"&.cm-focused": { outline: "none" },
	".cm-scroller": {
		fontFamily: "inherit",
		lineHeight: "1.25rem",
		maxHeight: "min(40vh, 20rem)",
		minHeight: "2.5rem",
		overflow: "auto",
	},
	".cm-content": { padding: "0.25rem 0.375rem", caretColor: "currentColor" },
	".cm-line": { padding: "0" },
	".cm-placeholder": { color: "var(--muted-foreground)" },
	// Vim's block cursor, in the text's colour.
	".cm-fat-cursor": { background: "var(--foreground) !important", color: "var(--background)" },
	"&:not(.cm-focused) .cm-fat-cursor": {
		background: "none !important",
		outline: "solid 1px var(--foreground)",
	},
});

export const PromptEditor = forwardRef<
	PromptEditorHandle,
	{
		value: string;
		onChange: (value: string) => void;
		onCursor?: (position: number | null) => void;
		/** Enter, from any mode (`alt`: with Alt held); Ctrl/⌘+Enter only, unless `enterSends`. */
		onSubmit: (alt: boolean) => void;
		/** Enter alone sends (not on touch screens, where it is a new line). */
		enterSends?: boolean;
		/** Files pasted (the text of a paste goes in as text). */
		onPasteFiles?: (files: File[]) => void;
		placeholder: string;
		label: string;
		disabled?: boolean;
		autoFocus?: boolean;
	}
>(function PromptEditor(
	{
		value,
		onChange,
		onCursor,
		onSubmit,
		onPasteFiles,
		placeholder,
		label,
		disabled,
		autoFocus,
		enterSends = true,
	},
	ref,
) {
	const host = useRef<HTMLDivElement>(null);
	const view = useRef<EditorView | null>(null);
	// Read by the editor's handlers, which outlive a render.
	const latest = useRef({ onChange, onSubmit, onPasteFiles, onCursor });
	latest.current = { onChange, onSubmit, onPasteFiles, onCursor };
	const config = useRef({ placeholder: new Compartment(), editable: new Compartment() });

	useImperativeHandle(
		ref,
		() => ({
			focus: () => view.current?.focus(),
			replaceRange: (from: number, to: number) =>
				view.current?.dispatch({ changes: { from, to, insert: "" }, selection: { anchor: from } }),
		}),
		[],
	);

	useEffect(() => {
		const parent = host.current;
		if (!parent) return;
		const vim = new Compartment();
		const created = new EditorView({
			parent,
			state: EditorState.create({
				doc: value,
				extensions: [
					vim.of([]),
					history(),
					drawSelection(),
					EditorView.lineWrapping,
					EditorView.contentAttributes.of({ "aria-label": label, "aria-multiline": "true" }),
					config.current.placeholder.of(placeholderText(placeholder)),
					config.current.editable.of(EditorView.editable.of(!disabled)),
					Prec.highest(keymap.of([{ key: "Shift-Enter", run: insertNewline }])),
					keymap.of([...defaultKeymap, ...historyKeymap]),
					EditorView.updateListener.of((update) => {
						if (update.docChanged || update.selectionSet)
							latest.current.onCursor?.(
								update.state.selection.main.empty ? update.state.selection.main.head : null,
							);
						if (update.docChanged) latest.current.onChange(update.state.doc.toString());
					}),
					EditorView.domEventHandlers({
						paste: (event) => {
							const files = [...(event.clipboardData?.files ?? [])];
							if (files.length === 0 || !latest.current.onPasteFiles) return false;
							event.preventDefault();
							latest.current.onPasteFiles(files);
							return true;
						},
					}),
					theme,
				],
			}),
		});
		view.current = created;
		if (autoFocus) created.focus();
		let cancelled = false;
		let stopFollowing: (() => void) | undefined;
		// The code editor (which loads Vim) stays out of the first download.
		void import("@/shared/editor/code-editor").then(async ({ followVim, loadVim }) => {
			const module = await loadVim();
			if (cancelled) return;
			created.dispatch({ effects: vim.reconfigure(module.vim()) });
			stopFollowing = followVim(module, created);
		});
		return () => {
			cancelled = true;
			stopFollowing?.();
			created.destroy();
			view.current = null;
		};
		// Made once; what changes later is applied below.
		// oxlint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	// A value set from outside (cleared once sent) replaces the text.
	useEffect(() => {
		const current = view.current;
		if (!current || current.state.doc.toString() === value) return;
		current.dispatch({ changes: { from: 0, to: current.state.doc.length, insert: value } });
	}, [value]);

	useEffect(() => {
		view.current?.dispatch({
			effects: [
				config.current.placeholder.reconfigure(placeholderText(placeholder)),
				config.current.editable.reconfigure(EditorView.editable.of(!disabled)),
			],
		});
	}, [placeholder, disabled]);

	return (
		<div
			ref={host}
			className="text-sm"
			// Before Vim and CodeMirror see it: Enter sends, from any mode.
			onKeyDownCapture={(event) => {
				const sends = enterSends || event.metaKey || event.ctrlKey;
				if (event.key === "Enter" && sends && !event.shiftKey && !event.nativeEvent.isComposing) {
					event.preventDefault();
					event.stopPropagation();
					latest.current.onSubmit(event.altKey);
				}
			}}
		/>
	);
});
