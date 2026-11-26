/**
 * The editor, on CodeMirror 6. Loaded with the first file window
 * (see `editor.tsx`), so the rest of the app does not carry it. Everything
 * CodeMirror-specific stays in this folder: the rest of Ahab speaks
 * `model.ts`'s terms.
 */

import { autocompletion, type CompletionContext } from "@codemirror/autocomplete";
import {
	copyLineDown,
	copyLineUp,
	defaultKeymap,
	deleteLine,
	history,
	historyKeymap,
	indentWithTab,
	moveLineDown,
	moveLineUp,
	toggleComment,
} from "@codemirror/commands";
import { foldGutter, foldKeymap, foldService, indentUnit } from "@codemirror/language";
import {
	gotoLine,
	highlightSelectionMatches,
	openSearchPanel,
	search,
	searchKeymap,
	selectNextOccurrence,
} from "@codemirror/search";
import {
	Annotation,
	type ChangeSet,
	Compartment,
	EditorSelection,
	EditorState,
	type Extension,
	StateEffect,
	StateField,
} from "@codemirror/state";
import {
	type BlockInfo,
	crosshairCursor,
	Decoration,
	type DecorationSet,
	drawSelection,
	dropCursor,
	EditorView,
	highlightActiveLine,
	highlightActiveLineGutter,
	highlightSpecialChars,
	hoverTooltip,
	type KeyBinding,
	showTooltip,
	type Tooltip,
	keymap,
	lineNumbers,
	rectangularSelection,
} from "@codemirror/view";
import { indentationMarkers } from "@replit/codemirror-indentation-markers";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

import {
	type CodeActionRequest,
	type CommentTokens,
	type DocumentChange,
	type DocumentSymbol,
	type EditorDocument,
	type EditorSettings,
	type HighlightSession,
	type HighlightToken,
	type Hover,
	type CursorInfo,
	indentFold,
	type LanguageService,
	type LineEnding,
	type LanguageStatus,
	type LineSelection,
	lineSelections,
	type Location,
	type LocationKind,
	type RenameRequest,
	type SignatureHelp,
	type TextEdit,
	type TextRange,
	separatorOf,
	visualColumn,
} from "@/shared/editor/model";
import {
	goToProblem,
	problemsAtLineEnd,
	problemsExtension,
	setProblems,
} from "@/shared/editor/problems";
import { openHighlighting, openLanguageService } from "@/shared/editor/registry";
import {
	pendingKeys,
	publishVimStatus,
	type VimMode,
	vimModeOf,
	type VimStatus,
} from "@/shared/editor/vim";
import { type EditorActionId, editorActionsFor, keyOf } from "@/shared/keymap/model";
import { keymap as currentKeymap } from "@/shared/keymap/store";
import {
	appliedVimMapping,
	currentVimMappings,
	subscribeVimMappings,
	type VimMapping,
} from "@/shared/keymap/vim";

/** What a file window asks of its editor. */
export interface EditorHandle {
	/** The text now. */
	text(): string;
	/** Replaces the text (a reload), keeping the selection and scroll where they still fit. */
	replace(text: string): void;
	focus(): void;
	/** The whole lines selected now. */
	selections(): LineSelection[];
	/**
	 * The outline: from the language service when it has one, else from the
	 * syntax tree; null when neither knows the language.
	 */
	symbols(): Promise<DocumentSymbol[] | null>;
	/** Selects `from`–`to` and shows it. */
	reveal(from: number, to: number): void;
	/** Goes to the definition (or others) of what is at the cursor. */
	navigate(kind: LocationKind): void;
	/** Which navigations the file's language tooling offers. */
	offers(kind: LocationKind | "rename" | "codeActions" | "format"): boolean;
	/** Formats the text (Edit mode); false when nothing formats it. */
	format(): Promise<boolean>;
	/** Whether the file is formatted before it is saved. */
	formatOnSave(): boolean;
	/** The file was saved. */
	saved(): void;
	/** F2 and ⌘.: rename, or list the fixes at the cursor. */
	rename(): void;
	codeActions(): void;
	/** Asks for a line (and column) to go to, in the editor's own prompt. */
	goToLine(): void;
	/** F8, ⇧F8: the next or previous problem in the file, shown; false when there are none. */
	goToProblem(direction: 1 | -1): boolean;
}

/** A selection to offer to a chat, and where its end shows (in the editor's box). */
export interface SelectionReport {
	lines: LineSelection[];
	top: number;
	left: number;
}

export interface CodeEditorProps {
	projectId: string;
	path: string;
	/** The text it opens with; later changes go through `replace`. */
	initial: string;
	readOnly: boolean;
	settings: EditorSettings;
	/**
	 * The language to show it as: a language's ID, `plaintext`, or
	 * absent for the one the highlighter finds itself.
	 */
	language?: string;
	/** A line to show and mark, again each time `jump` changes. */
	line?: number;
	jump?: number;
	/** Fills its box (a window), or grows with its text (beside a preview). */
	fill?: boolean;
	onChange?: (text: string) => void;
	/** The selection changed: whole lines, or null when nothing is selected. */
	onSelect?: (selection: SelectionReport | null) => void;
	onSave?: () => void;
	onToggleEdit?: () => void;
	onAddToChat?: (lines: LineSelection[]) => void;
	/** ⇧⌘O: show the outline. */
	onOutline?: () => void;
	onChooseChat?: (lines: LineSelection[]) => void;
	/** Go to definition and references found places. */
	onLocations?: (locations: Location[], kind: LocationKind) => void;
	/** What serves the file changed. */
	onLanguageStatus?: (status: LanguageStatus | null) => void;
	/** F2: ask for the new name and preview the rename. */
	onRename?: (request: RenameRequest) => void;
	/** ⌘.: list the fixes offered. */
	onCodeActions?: (request: CodeActionRequest) => void;
	/** The editor lost the focus (auto-save on focus change). */
	onBlur?: () => void;
	/**
	 * How lines end in the text it reports (`text()`, `onChange`); the editor
	 * itself works in LF. LF when absent.
	 */
	lineEnding?: LineEnding;
	/** The cursor moved or the selection changed (the status bar). */
	onCursor?: (cursor: CursorInfo) => void;
}

/** The text with lines ending as asked. */
function textOf(state: EditorState, ending: LineEnding | undefined): string {
	return state.doc.sliceString(0, state.doc.length, separatorOf(ending ?? "LF"));
}

/** Where the main cursor is, and what is selected. */
function cursorOf(state: EditorState): CursorInfo {
	const { main, ranges } = state.selection;
	const line = state.doc.lineAt(main.head);
	return {
		line: line.number,
		column: visualColumn(line.text, main.head - line.from, state.tabSize),
		selected: ranges.reduce((sum, range) => sum + (range.to - range.from), 0),
		cursors: ranges.length,
	};
}

// --- Highlighting from an external engine -------------------------

const setTokens = StateEffect.define<{
	decorations: DecorationSet;
	range?: { from: number; to: number };
}>();

const tokens = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(value, transaction) {
		let next = value.map(transaction.changes);
		for (const effect of transaction.effects) {
			if (!effect.is(setTokens)) continue;
			const { decorations, range } = effect.value;
			if (!range) next = decorations;
			else {
				const added: { from: number; to: number; value: Decoration }[] = [];
				decorations.between(range.from, range.to, (from, to, decoration) => {
					added.push({ from, to, value: decoration });
				});
				next = next.update({
					filter: (from, to) => to <= range.from || from >= range.to,
					add: added.map((item) => item.value.range(item.from, item.to)),
					sort: true,
				});
			}
		}
		return next;
	},
	provide: (field) => EditorView.decorations.from(field),
});

const marks = new Map<string, Decoration>();
function mark(tag: string): Decoration {
	let found = marks.get(tag);
	if (!found) {
		const parts = tag.split(".");
		const classes = parts.map((_, index) => `tok-${parts.slice(0, index + 1).join("-")}`);
		found = Decoration.mark({ class: classes.join(" ") });
		marks.set(tag, found);
	}
	return found;
}

function decorate(list: HighlightToken[], length: number): DecorationSet {
	const ranges = list
		.filter((token) => token.to > token.from && token.from >= 0 && token.to <= length)
		.toSorted((a, b) => a.from - b.from || a.to - b.to)
		.map((token) => mark(token.tag).range(token.from, token.to));
	return Decoration.set(ranges, true);
}

// --- Folding from the syntax tree -----------------------------------

const setFolds = StateEffect.define<{ folds: TextRange[]; changes: ChangeSet[] } | null>();

/** The engine's folds, kept in step with edits; null until it sends some (then indentation folds). */
const syntaxFolds = StateField.define<TextRange[] | null>({
	create: () => null,
	update(value, transaction) {
		let next = value;
		if (next && transaction.docChanged)
			next = next
				.map((fold) => ({
					from: transaction.changes.mapPos(fold.from, 1),
					to: transaction.changes.mapPos(fold.to, -1),
				}))
				.filter((fold) => fold.to > fold.from);
		for (const effect of transaction.effects) {
			if (!effect.is(setFolds)) continue;
			if (!effect.value) next = null;
			else {
				const { changes } = effect.value;
				next = effect.value.folds
					.map((fold) => {
						let { from, to } = fold;
						for (const change of changes) {
							from = change.mapPos(from, 1);
							to = change.mapPos(to, -1);
						}
						return { from, to };
					})
					.filter((fold) => fold.to > fold.from);
			}
		}
		return next;
	},
});

function foldAt(state: EditorState, lineStart: number, lineEnd: number) {
	const folds = state.field(syntaxFolds, false);
	if (folds) {
		const found = folds.find((fold) => fold.from >= lineStart && fold.from <= lineEnd);
		return found && found.to <= state.doc.length ? { from: lineEnd, to: found.to } : null;
	}
	const doc = state.doc;
	const line = doc.lineAt(lineStart);
	const fold = indentFold((n) => doc.line(n).text, doc.lines, line.number, state.tabSize);
	return fold ? { from: line.to, to: doc.line(fold.endLine).to } : null;
}

/** Marks a change that came from outside (a reload), not from typing. */
const external = Annotation.define<boolean>();

// --- Marking a line asked for -------------------------------------------------

const flash = StateEffect.define<number | null>();
const flashed = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(value, transaction) {
		let next = value.map(transaction.changes);
		for (const effect of transaction.effects) {
			if (!effect.is(flash)) continue;
			next =
				effect.value === null
					? Decoration.none
					: Decoration.set([Decoration.line({ class: "cm-ahab-flash" }).range(effect.value)]);
		}
		return next;
	},
	provide: (field) => EditorView.decorations.from(field),
});

// --- Selecting whole lines from the line numbers -------------------------------

function lineAtY(view: EditorView, y: number) {
	const pos = view.posAtCoords({ x: view.contentDOM.getBoundingClientRect().left + 2, y }, false);
	return view.state.doc.lineAt(pos);
}

/** Dragging in the line numbers selects whole lines; Shift extends. */
function gutterSelect(view: EditorView, block: BlockInfo, event: Event): boolean {
	if (!(event instanceof MouseEvent) || event.button !== 0) return false;
	const line = view.state.doc.lineAt(block.from);
	const anchorLine = event.shiftKey
		? view.state.doc.lineAt(view.state.selection.main.anchor)
		: line;
	const select = (to: { from: number; to: number; number: number }) => {
		const down = to.number >= anchorLine.number;
		const start = down ? anchorLine : to;
		const end = down ? to : anchorLine;
		const endPos = Math.min(end.to + 1, view.state.doc.length);
		view.dispatch({
			selection: down
				? EditorSelection.single(start.from, endPos)
				: EditorSelection.single(endPos, start.from),
		});
	};
	select(line);
	const move = (moved: MouseEvent) => select(lineAtY(view, moved.clientY));
	const up = () => {
		window.removeEventListener("mousemove", move);
		window.removeEventListener("mouseup", up);
	};
	window.addEventListener("mousemove", move);
	window.addEventListener("mouseup", up);
	view.focus();
	event.preventDefault();
	return true;
}

// --- The look ---------------------------------------------------------------------

const theme = EditorView.theme({
	"&": {
		height: "100%",
		color: "var(--foreground)",
		backgroundColor: "var(--background)",
		fontSize: "calc(var(--ahab-editor-font-size, 13px) * var(--ahab-text-scale, 1))",
	},
	"&.cm-focused": { outline: "none" },
	".cm-scroller": {
		fontFamily: "var(--font-mono, ui-monospace, monospace)",
		lineHeight: "1.55",
	},
	".cm-content": { caretColor: "var(--primary)", paddingBlock: "8px" },
	".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--primary)" },
	".cm-gutters": {
		backgroundColor: "var(--background)",
		color: "color-mix(in srgb, var(--muted-foreground) 70%, transparent)",
		border: "none",
	},
	".cm-lineNumbers .cm-gutterElement": {
		paddingLeft: "12px",
		paddingRight: "12px",
		cursor: "default",
		userSelect: "none",
	},
	".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--accent) 45%, transparent)" },
	".cm-activeLineGutter": {
		backgroundColor: "transparent",
		color: "var(--foreground)",
	},
	"&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
		{ backgroundColor: "color-mix(in srgb, var(--primary) 30%, transparent) !important" },
	".cm-selectionMatch": { backgroundColor: "color-mix(in srgb, var(--primary) 16%, transparent)" },
	".cm-searchMatch": {
		backgroundColor: "color-mix(in srgb, var(--warning) 30%, transparent)",
		outline: "1px solid color-mix(in srgb, var(--warning) 50%, transparent)",
	},
	".cm-searchMatch-selected": {
		backgroundColor: "color-mix(in srgb, var(--warning) 55%, transparent)",
	},
	".cm-foldPlaceholder": {
		backgroundColor: "var(--muted)",
		border: "1px solid var(--border)",
		color: "var(--muted-foreground)",
	},
	".cm-panels": {
		backgroundColor: "var(--card)",
		color: "var(--foreground)",
	},
	".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
	".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
	".cm-panel input, .cm-panel button": { fontSize: "12px" },
	".cm-textfield": {
		backgroundColor: "var(--background)",
		border: "1px solid var(--input)",
		borderRadius: "4px",
		color: "var(--foreground)",
	},
	".cm-button": {
		backgroundImage: "none",
		backgroundColor: "var(--secondary)",
		border: "1px solid var(--input)",
		borderRadius: "4px",
		color: "var(--foreground)",
	},
	".cm-tooltip": {
		backgroundColor: "var(--popover)",
		color: "var(--popover-foreground)",
		border: "1px solid var(--border)",
		borderRadius: "6px",
	},
	".cm-tooltip-ahab-hover, .cm-tooltip-ahab-signature": {
		maxWidth: "36rem",
		maxHeight: "20rem",
		overflow: "auto",
		padding: "6px 8px",
		whiteSpace: "pre-wrap",
		fontSize: "12px",
	},
	".cm-ahab-hover-code": {
		fontFamily: "var(--font-mono)",
		margin: "0 0 4px",
		whiteSpace: "pre-wrap",
	},
	".cm-ahab-hover-text + .cm-ahab-hover-code, .cm-ahab-hover-code + .cm-ahab-hover-text": {
		borderTop: "1px solid var(--border)",
		paddingTop: "4px",
	},
	".cm-tooltip-ahab-doc": { color: "var(--muted-foreground)", marginTop: "4px" },
	".cm-ahab-highlight": { backgroundColor: "color-mix(in srgb, var(--primary) 16%, transparent)" },
	".cm-ahab-flash": { backgroundColor: "color-mix(in srgb, var(--warning) 22%, transparent)" },
});

function settingsExtensions(settings: EditorSettings, fill: boolean): Extension[] {
	return [
		EditorView.editorAttributes.of({
			style: `--ahab-editor-font-size: ${settings.fontSize}px`,
		}),
		settings.lineWrapping ? EditorView.lineWrapping : [],
		settings.indentationGuides
			? indentationMarkers({
					colors: {
						light: "#00000014",
						dark: "#ffffff14",
						activeLight: "#00000030",
						activeDark: "#ffffff30",
					},
				})
			: [],
		EditorState.tabSize.of(settings.tabSize),
		problemsAtLineEnd.of(Boolean(settings.problemsAtLineEnd)),
		indentUnit.of(settings.useTabs ? "\t" : " ".repeat(settings.tabSize)),
		fill
			? []
			: EditorView.theme({ "&": { height: "auto" }, ".cm-scroller": { overflow: "visible" } }),
	];
}

// --- Commands, run by keys (Settings → Keyboard) and by Vim -------------------

/** Runs an editor command: false when it does not apply here, so the key goes on. */
type Run = (view: EditorView) => boolean;
type Commands = Partial<Record<EditorActionId, Run>>;

/** The editor's commands that are CodeMirror's own, the same for every file. */
const BUILT_IN: Commands = {
	"editor.find": openSearchPanel,
	"editor.goToLine": gotoLine,
	"editor.selectNextOccurrence": selectNextOccurrence,
	"editor.deleteLine": deleteLine,
	"editor.moveLineUp": moveLineUp,
	"editor.moveLineDown": moveLineDown,
	"editor.copyLineUp": copyLineUp,
	"editor.copyLineDown": copyLineDown,
	"editor.toggleComment": toggleComment,
};

const OWNED = new Set<unknown>(Object.values(BUILT_IN));

/** CodeMirror's bindings, without those of commands whose keys Settings → Keyboard sets. */
function unowned(bindings: readonly KeyBinding[]): KeyBinding[] {
	return bindings.filter((binding) => !OWNED.has(binding.run));
}

/**
 * The editor's keys, as Settings → Keyboard sets them (the keymap in use is
 * read at each key): each key's commands are tried in turn, the first that
 * applies wins. After Vim's keys, which come first while Vim is on.
 */
function editorKeys(commands: () => Commands): Extension {
	return EditorView.domEventHandlers({
		keydown(event, view) {
			const key = keyOf(event);
			if (!key) return false;
			for (const id of editorActionsFor(currentKeymap(), key)) {
				if (commands()[id]?.(view)) {
					event.preventDefault();
					return true;
				}
			}
			return false;
		},
	});
}

// --- Vim, loaded when asked for -----------------------------------------------

type VimModule = typeof import("@replit/codemirror-vim");
let vimModule: Promise<VimModule> | undefined;

/** What Vim's own keys reach in each view: its save (`:w`) and its commands (`gd`). */
interface VimHooks {
	save(): void;
	run(id: EditorActionId): boolean;
}
const vimHooks = new WeakMap<EditorView, VimHooks>();

/**
 * Vim's keys for the language tools, as VS Code's Vim and Helix have them.
 * `gi` keeps Vim's own meaning (insert where the last change was) where
 * nothing finds implementations; `gc` and `gcc` are Vim's own (commenting).
 */
const VIM_COMMANDS: readonly { keys: string; id: EditorActionId }[] = [
	{ keys: "gd", id: "editor.goToDefinition" },
	{ keys: "gy", id: "editor.goToTypeDefinition" },
	{ keys: "gi", id: "editor.goToImplementation" },
	{ keys: "gr", id: "editor.goToReferences" },
	{ keys: "K", id: "editor.showHover" },
	{ keys: "gh", id: "editor.showHover" },
	{ keys: "]d", id: "editor.nextProblem" },
	{ keys: "[d", id: "editor.previousProblem" },
];

/** The mappings of Settings → Keyboard applied now, to take back when they change. */
let appliedMappings: VimMapping[] = [];

/** `:map` applies in normal, visual and operator-pending mode: no context. */
const contextOf = (mapping: VimMapping) =>
	(mapping.mode === "all" ? undefined : mapping.mode) as string;

/** Applies this device's mappings, in place of the ones applied before. */
function applyVimMappings(Vim: VimModule["Vim"]) {
	for (const mapping of appliedMappings) Vim.unmap(mapping.lhs, contextOf(mapping));
	appliedMappings = [];
	const list = currentVimMappings();
	list.forEach((mapping, index) => {
		const ready = appliedVimMapping(mapping, list.slice(0, index));
		if (!ready) return;
		if (ready.recursive) Vim.map(ready.lhs, ready.rhs, contextOf(ready));
		else Vim.noremap(ready.lhs, ready.rhs, contextOf(ready));
		appliedMappings.push(ready);
	});
}

/** Vim's `:w`: the view's own save. */
function vimWrite(cm: { cm6?: EditorView }) {
	if (cm.cm6) vimHooks.get(cm.cm6)?.save();
}

export function loadVim(): Promise<VimModule> {
	vimModule ??= import("@replit/codemirror-vim").then((module) => {
		const { Vim } = module;
		Vim.defineEx("write", "w", vimWrite);
		Vim.defineEx("update", "up", vimWrite);
		for (const { keys, id } of VIM_COMMANDS) {
			const name = `ahab ${keys}`;
			Vim.defineAction(name, (cm) => {
				const view = cm.cm6 as EditorView | undefined;
				if (view && vimHooks.get(view)?.run(id)) return;
				if (keys !== "gi") return;
				// Vim's own `gi`: insert where the last change was.
				const at = cm.getLastEditEnd();
				if (at) cm.setCursor(at);
				Vim.handleKey(cm, "i", "mapping");
			});
			Vim.mapCommand(keys, "action", name, {}, { context: "normal" });
		}
		applyVimMappings(Vim);
		subscribeVimMappings(() => applyVimMappings(Vim));
		return module;
	});
	return vimModule;
}

/**
 * Follows a view's Vim for the status bar: its mode, and the keys of the
 * command being typed, published while the view has the focus. Answers
 * how to stop.
 */
export function followVim(module: VimModule, view: EditorView): () => void {
	const cm = module.getCM(view);
	if (!cm) return () => undefined;
	let mode: VimMode = "normal";
	let typed = "";
	const publish = () => {
		if (!view.hasFocus) return;
		const pending = pendingKeys(mode, typed, cm.state.vim?.inputState);
		if (!pending) typed = "";
		const status: VimStatus = { mode, pending };
		publishVimStatus(view, status);
	};
	const onMode = (event: { mode?: string; subMode?: string }) => {
		mode = vimModeOf(event);
		typed = "";
		publish();
	};
	const onKey = (key: string) => {
		typed += key;
		publish();
	};
	const onDone = () => {
		typed = "";
		publish();
	};
	cm.on("vim-mode-change", onMode);
	cm.on("vim-keypress", onKey);
	cm.on("vim-command-done", onDone);
	view.contentDOM.addEventListener("focus", publish);
	publish();
	return () => {
		cm.off("vim-mode-change", onMode);
		cm.off("vim-keypress", onKey);
		cm.off("vim-command-done", onDone);
		view.contentDOM.removeEventListener("focus", publish);
		publishVimStatus(view, null);
	};
}

// --- Language tooling -----------------------------------------------

/** What the window does with what the language service found. */
interface LanguageHooks {
	locations: (list: Location[], kind: LocationKind) => void;
	rename?: (request: RenameRequest) => void;
	codeActions?: (request: CodeActionRequest) => void;
}

const setHighlights = StateEffect.define<TextRange[]>();

/** The same symbol's places, marked while the cursor is on it. */
const highlights = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(value, transaction) {
		let next = value.map(transaction.changes);
		for (const effect of transaction.effects) {
			if (!effect.is(setHighlights)) continue;
			const length = transaction.state.doc.length;
			next = Decoration.set(
				effect.value
					.filter((range) => range.to <= length && range.from < range.to)
					.map((range) =>
						Decoration.mark({ class: "cm-ahab-highlight" }).range(range.from, range.to),
					),
				true,
			);
		}
		return next;
	},
	provide: (field) => EditorView.decorations.from(field),
});

const setSignature = StateEffect.define<{ pos: number; help: SignatureHelp } | null>();

/** A function's signature while its arguments are typed. */
const signature = StateField.define<Tooltip | null>({
	create: () => null,
	update(value, transaction) {
		let next = value;
		if (next && transaction.docChanged)
			next = { ...next, pos: transaction.changes.mapPos(next.pos) };
		if (next && transaction.selection) {
			const head = transaction.state.selection.main.head;
			const line = transaction.state.doc.lineAt(next.pos);
			if (head < next.pos - 200 || head > line.to) next = null;
		}
		for (const effect of transaction.effects) {
			if (!effect.is(setSignature)) continue;
			const shown = effect.value;
			next = shown
				? {
						pos: shown.pos,
						above: true,
						create: () => ({ dom: signatureDom(shown.help) }),
					}
				: null;
		}
		return next;
	},
	provide: (field) => showTooltip.from(field),
});

function signatureDom(help: SignatureHelp): HTMLElement {
	const dom = document.createElement("div");
	dom.className = "cm-tooltip-ahab-signature";
	const label = document.createElement("div");
	label.className = "font-mono";
	if (help.active) {
		const [from, to] = help.active;
		const strong = document.createElement("strong");
		strong.textContent = help.label.slice(from, to);
		label.append(help.label.slice(0, from), strong, help.label.slice(to));
	} else label.textContent = help.label;
	dom.append(label);
	if (help.documentation) {
		const doc = document.createElement("div");
		doc.className = "cm-tooltip-ahab-doc";
		doc.textContent = help.documentation;
		dom.append(doc);
	}
	return dom;
}

/** Markdown from a hover, as plain blocks: code fences as code, the rest as text. */
function markdownDom(markdown: string): HTMLElement {
	const dom = document.createElement("div");
	dom.className = "cm-tooltip-ahab-hover";
	const parts = markdown.split(/^```[^\n]*\n?/m);
	parts.forEach((part, index) => {
		const text = part.replace(/\n+$/, "");
		if (!text.trim()) return;
		const block = document.createElement(index % 2 === 1 ? "pre" : "div");
		block.className = index % 2 === 1 ? "cm-ahab-hover-code" : "cm-ahab-hover-text";
		block.textContent = index % 2 === 1 ? text : text.replace(/[*_`]/g, "");
		dom.append(block);
	});
	return dom;
}

const setPinnedHover = StateEffect.define<Hover | null>();

/** `K`, or Show the hover: the hover at the cursor, until it moves, the text changes, or Escape. */
const pinnedHover = StateField.define<Tooltip | null>({
	create: () => null,
	update(value, transaction) {
		for (const effect of transaction.effects) {
			if (!effect.is(setPinnedHover)) continue;
			const hover = effect.value;
			return hover && hover.to <= transaction.state.doc.length
				? {
						pos: hover.from,
						end: hover.to,
						above: true,
						create: () => ({ dom: markdownDom(hover.markdown) }),
					}
				: null;
		}
		return value && (transaction.docChanged || transaction.selection) ? null : value;
	},
	provide: (field) => showTooltip.from(field),
});

const closePinnedHover = keymap.of([
	{
		key: "Escape",
		run: (view) => {
			if (!view.state.field(pinnedHover, false)) return false;
			view.dispatch({ effects: setPinnedHover.of(null) });
			return true;
		},
	},
]);

/** What a file's language tooling adds: extensions, and the commands it can run. */
function languageExtensions(
	service: LanguageService | null,
	readOnly: boolean,
	hooks: LanguageHooks,
): { extensions: Extension[]; commands: Commands } {
	if (!service) return { extensions: [], commands: {} };
	const found: Extension[] = [];
	const commands: Commands = {};
	if (service.diagnostics) {
		const actions = service.codeActions?.bind(service);
		const applyFixes = service.applyEdits?.bind(service);
		found.push(
			problemsExtension(
				actions && applyFixes ? { find: actions, apply: applyFixes, readOnly } : undefined,
			),
		);
		commands["editor.nextProblem"] = (view) => goToProblem(view, 1);
		commands["editor.previousProblem"] = (view) => goToProblem(view, -1);
	}
	if (service.hover) {
		const hover = service.hover.bind(service);
		found.push(pinnedHover, closePinnedHover);
		commands["editor.showHover"] = (view) => {
			const head = view.state.selection.main.head;
			void hover(head).then((answer) => {
				if (view.state.selection.main.head === head)
					view.dispatch({ effects: setPinnedHover.of(answer) });
			});
			return true;
		};
		found.push(
			hoverTooltip(
				async (_view, pos) => {
					const answer = await hover(pos);
					if (!answer) return null;
					return {
						pos: answer.from,
						end: answer.to,
						create: () => ({ dom: markdownDom(answer.markdown) }),
					};
				},
				{ hoverTime: 400 },
			),
		);
	}
	const navigate = (
		kind: LocationKind,
		ask: ((offset: number) => Promise<Location[]>) | undefined,
	) =>
		ask
			? (view: EditorView) => {
					void ask(view.state.selection.main.head).then((list) => {
						if (list.length) hooks.locations(list, kind);
					});
					return true;
				}
			: undefined;
	const definition = navigate("definition", service.definition?.bind(service));
	if (definition) {
		commands["editor.goToDefinition"] = definition;
		// ⌘-click or Ctrl-click goes to the definition.
		found.push(
			EditorView.domEventHandlers({
				mousedown(event, view) {
					if (!(event.metaKey || event.ctrlKey) || event.button !== 0) return false;
					const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
					if (pos == null) return false;
					view.dispatch({ selection: EditorSelection.cursor(pos) });
					return definition(view);
				},
			}),
		);
	}
	const references = navigate("references", service.references?.bind(service));
	if (references) commands["editor.goToReferences"] = references;
	const typeDefinition = navigate("typeDefinition", service.typeDefinition?.bind(service));
	if (typeDefinition) commands["editor.goToTypeDefinition"] = typeDefinition;
	const implementation = navigate("implementation", service.implementation?.bind(service));
	if (implementation) commands["editor.goToImplementation"] = implementation;
	if (service.highlights) {
		const ask = service.highlights.bind(service);
		let timer: ReturnType<typeof setTimeout> | undefined;
		found.push(
			highlights,
			EditorView.updateListener.of((update) => {
				if (!update.selectionSet && !update.docChanged) return;
				clearTimeout(timer);
				const view = update.view;
				const head = update.state.selection.main.head;
				if (update.docChanged) view.dispatch({ effects: setHighlights.of([]) });
				timer = setTimeout(() => {
					void ask(head).then((ranges) => {
						if (view.state.selection.main.head === head)
							view.dispatch({ effects: setHighlights.of(ranges) });
					});
				}, 250);
			}),
		);
	}
	if (!readOnly && service.format) {
		const format = service.format.bind(service);
		commands["editor.format"] = (view) => {
			void format().then((edits) => {
				if (edits.length) view.dispatch({ changes: edits.map(toChange) });
			});
			return true;
		};
	}
	const apply = service.applyEdits?.bind(service);
	if (!readOnly && service.prepareRename && service.rename && apply && hooks.rename) {
		const prepare = service.prepareRename.bind(service);
		const rename = service.rename.bind(service);
		const ask = hooks.rename;
		commands["editor.rename"] = (view) => {
			const offset = view.state.selection.main.head;
			void prepare(offset).then((target) => {
				if (target) ask({ name: target.name, rename: (name) => rename(offset, name), apply });
			});
			return true;
		};
	}
	if (!readOnly && service.codeActions && apply && hooks.codeActions) {
		const actionsAt = service.codeActions.bind(service);
		const show = hooks.codeActions;
		commands["editor.codeActions"] = (view) => {
			const { from, to } = view.state.selection.main;
			void actionsAt(from, to).then((actions) => show({ actions, apply }));
			return true;
		};
	}
	if (!readOnly && service.signatureHelp) {
		const help = service.signatureHelp.bind(service);
		found.push(
			signature,
			EditorView.updateListener.of((update) => {
				if (!update.docChanged) return;
				let typed = "";
				update.changes.iterChanges((_fa, _ta, _fb, _tb, inserted) => {
					typed = inserted.toString();
				});
				const view = update.view;
				if (typed === ")") {
					view.dispatch({ effects: setSignature.of(null) });
					return;
				}
				if (typed !== "(" && typed !== ",") return;
				const pos = update.state.selection.main.head;
				void help(pos).then((answer) => {
					if (view.state.selection.main.head !== pos) return;
					view.dispatch({ effects: setSignature.of(answer ? { pos, help: answer } : null) });
				});
			}),
		);
	}
	if (!readOnly && service.completion) {
		const complete = service.completion.bind(service);
		found.push(
			autocompletion({
				override: [
					async (context: CompletionContext) => {
						const answer = await complete(context.pos, context.explicit);
						if (!answer) return null;
						return {
							from: answer.from,
							options: answer.items.map((item) => ({
								label: item.label,
								detail: item.detail,
								info: item.info,
								apply: item.insert,
								type: item.kind,
							})),
						};
					},
				],
			}),
		);
	}
	return { extensions: found, commands };
}

/** How the language writes comments, as CodeMirror's commenting reads it. */
function commentData(comments: CommentTokens | undefined) {
	if (!comments) return [];
	const block = comments.block ? { open: comments.block[0], close: comments.block[1] } : undefined;
	return [{ commentTokens: { line: comments.line, block } }];
}

function toChange(edit: TextEdit) {
	return { from: edit.from, to: edit.to, insert: edit.insert };
}

function editsOf(changes: ChangeSet): TextEdit[] {
	const edits: TextEdit[] = [];
	changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
		edits.push({ from: fromA, to: toA, insert: inserted.toString() });
	});
	return edits;
}

// --- The component ---------------------------------------------------------------

/** How many changes back tokens from a slow engine can still be mapped. */
const KEPT_CHANGES = 200;

export const CodeEditor = forwardRef<EditorHandle, CodeEditorProps>(
	function CodeEditor(props, ref) {
		const box = useRef<HTMLDivElement>(null);
		const view = useRef<EditorView | null>(null);
		const latest = useRef(props);
		latest.current = props;
		const compartments = useRef({
			readOnly: new Compartment(),
			settings: new Compartment(),
			lines: new Compartment(),
			vim: new Compartment(),
			language: new Compartment(),
		});
		// The document's version, and the changes since earlier ones.
		const versions = useRef({ version: 0, changes: new Map<number, ChangeSet>() });
		const pointerDown = useRef(false);
		const restartLanguage = useRef<((readOnly: boolean) => void) | undefined>(undefined);
		const languageRef = useRef<LanguageService | null>(null);
		const restartHighlighting = useRef<(() => void) | undefined>(undefined);
		const symbolsOf = useRef<(() => Promise<DocumentSymbol[] | null>) | undefined>(undefined);
		// The commands keys and Vim run here: CodeMirror's, the window's, the language's.
		const commands = useRef<Commands>(BUILT_IN);
		const { projectId, path, readOnly, settings, fill = true, language } = props;

		const report = (target: EditorView) => {
			const onSelect = latest.current.onSelect;
			if (!onSelect) return;
			const { state } = target;
			const lines = lineSelections(state.doc, state.selection.ranges);
			if (!lines.length || pointerDown.current) return onSelect(null);
			const head = state.selection.main.head;
			const coords = target.coordsAtPos(head) ?? target.coordsAtPos(state.selection.main.to);
			const outer = box.current?.getBoundingClientRect();
			if (!coords || !outer) return onSelect(null);
			onSelect({
				lines,
				top: Math.max(0, coords.bottom - outer.top + 4),
				left: Math.max(0, Math.min(coords.left - outer.left, outer.width - 240)),
			});
		};

		// The editor, made once per file.
		useEffect(() => {
			const parent = box.current;
			if (!parent) return;
			const { readOnly: ro, settings: initialSettings, fill: fills = true } = latest.current;
			const c = compartments.current;
			let highlightSession: HighlightSession | null = null;
			let languageService: LanguageService | null = null;
			let stopDiagnostics: (() => void) | undefined;
			let stopStatus: (() => void) | undefined;
			const lines = () =>
				view.current
					? lineSelections(view.current.state.doc, view.current.state.selection.ranges)
					: [];
			// Expanding the selection by syntax: each step the node
			// around it; shrinking goes back the way it came.
			const grown: { from: number; to: number }[] = [];
			let growing = false;
			const expand = (target: EditorView) => {
				const session = highlightSession;
				if (!session?.selectionRanges) return false;
				const { from, to } = target.state.selection.main;
				const version = versions.current.version;
				void session.selectionRanges(from, to).then((ranges) => {
					if (view.current !== target || versions.current.version !== version) return;
					const now = target.state.selection.main;
					const next = ranges.find(
						(range) =>
							range.from <= now.from &&
							range.to >= now.to &&
							(range.from < now.from || range.to > now.to) &&
							range.to <= target.state.doc.length,
					);
					if (!next) return;
					grown.push({ from: now.from, to: now.to });
					growing = true;
					target.dispatch({
						selection: EditorSelection.single(next.from, next.to),
						scrollIntoView: true,
					});
					growing = false;
				});
				return true;
			};
			const shrink = (target: EditorView) => {
				const last = grown.pop();
				if (!last) return false;
				growing = true;
				target.dispatch({
					selection: EditorSelection.single(last.from, last.to),
					scrollIntoView: true,
				});
				growing = false;
				return true;
			};
			const ahabKeys: KeyBinding[] = [
				{
					key: "Shift-Alt-ArrowRight",
					mac: "Ctrl-Shift-Mod-ArrowRight",
					run: expand,
					preventDefault: true,
				},
				{
					key: "Shift-Alt-ArrowLeft",
					mac: "Ctrl-Shift-Mod-ArrowLeft",
					run: shrink,
					preventDefault: true,
				},
				{ key: "Mod-i", run: expand, preventDefault: true },
				{
					key: "Shift-Mod-o",
					run: () => {
						if (!latest.current.onOutline) return false;
						latest.current.onOutline();
						return true;
					},
					preventDefault: true,
				},
				{
					key: "Mod-l",
					run: () => {
						const found = lines();
						if (!found.length || !latest.current.onAddToChat) return false;
						latest.current.onAddToChat(found);
						return true;
					},
					preventDefault: true,
				},
				{
					key: "Shift-Mod-l",
					run: () => {
						const found = lines();
						if (!found.length || !latest.current.onChooseChat) return false;
						latest.current.onChooseChat(found);
						return true;
					},
					preventDefault: true,
				},
			];
			const windowCommands: Commands = {
				...BUILT_IN,
				"editor.save": () => {
					latest.current.onSave?.();
					return true;
				},
				"editor.toggleEdit": () => {
					latest.current.onToggleEdit?.();
					return true;
				},
			};
			commands.current = windowCommands;
			const state = EditorState.create({
				doc: latest.current.initial,
				extensions: [
					c.vim.of([]),
					editorKeys(() => commands.current),
					EditorState.languageData.of(() => commentData(highlightSession?.comments?.())),
					c.lines.of(
						initialSettings.lineNumbers
							? lineNumbers({ domEventHandlers: { mousedown: gutterSelect } })
							: [],
					),
					highlightActiveLineGutter(),
					highlightSpecialChars(),
					history(),
					foldGutter({
						foldingChanged: (update) =>
							update.transactions.some((tr) => tr.effects.some((effect) => effect.is(setFolds))),
					}),
					syntaxFolds,
					foldService.of(foldAt),
					drawSelection(),
					dropCursor(),
					EditorState.allowMultipleSelections.of(true),
					rectangularSelection(),
					crosshairCursor(),
					highlightActiveLine(),
					highlightSelectionMatches(),
					search({ top: true }),
					keymap.of([
						...ahabKeys,
						...unowned(searchKeymap),
						...unowned(defaultKeymap),
						...historyKeymap,
						...foldKeymap,
						indentWithTab,
					]),
					c.readOnly.of(EditorState.readOnly.of(ro)),
					c.settings.of(settingsExtensions(initialSettings, fills)),
					c.language.of([]),
					tokens,
					flashed,
					theme,
					EditorView.updateListener.of((update) => {
						if (update.docChanged) {
							const tracked = versions.current;
							tracked.changes.set(tracked.version, update.changes);
							tracked.changes.delete(tracked.version - KEPT_CHANGES);
							tracked.version += 1;
							const change: DocumentChange = {
								version: tracked.version,
								edits: editsOf(update.changes),
							};
							highlightSession?.update(change);
							languageService?.update?.(change);
							if (!update.transactions.some((tr) => tr.annotation(external)))
								latest.current.onChange?.(textOf(update.state, latest.current.lineEnding));
						}
						// Another selection: expanding starts again from it.
						if ((update.selectionSet || update.docChanged) && !growing) grown.length = 0;
						if (update.selectionSet || update.docChanged || update.geometryChanged)
							report(update.view);
						if (
							update.selectionSet ||
							update.docChanged ||
							update.startState.tabSize !== update.state.tabSize
						)
							latest.current.onCursor?.(cursorOf(update.state));
						if (update.focusChanged && !update.view.hasFocus) latest.current.onBlur?.();
					}),
					EditorView.domEventHandlers({
						mousedown: () => {
							pointerDown.current = true;
							const up = () => {
								pointerDown.current = false;
								window.removeEventListener("mouseup", up);
								if (view.current) report(view.current);
							};
							window.addEventListener("mouseup", up);
							return false;
						},
						scroll: (_event, target) => {
							report(target);
							return false;
						},
					}),
				],
			});
			const created = new EditorView({ state, parent });
			view.current = created;
			latest.current.onCursor?.(cursorOf(created.state));
			vimHooks.set(created, {
				save: () => latest.current.onSave?.(),
				run: (id) => Boolean(commands.current[id]?.(created)),
			});

			// Highlighting and tooling for this document.
			const handle: EditorDocument = {
				path: latest.current.path,
				text: () => created.state.doc.toString(),
				get version() {
					return versions.current.version;
				},
				get languageId() {
					return latest.current.language;
				},
				positionAt: (offset) => {
					const line = created.state.doc.lineAt(offset);
					return { line: line.number - 1, character: offset - line.from };
				},
				offsetAt: ({ line, character }) => {
					const doc = created.state.doc;
					const found = doc.line(Math.min(Math.max(line + 1, 1), doc.lines));
					return Math.min(found.from + character, found.to);
				},
			};
			/** The edits since `version`, or null when they are no longer kept. */
			const since = (version: number): ChangeSet[] | null => {
				const list: ChangeSet[] = [];
				for (let at = version; at < versions.current.version; at++) {
					const changes = versions.current.changes.get(at);
					if (!changes) return null;
					list.push(changes);
				}
				return list;
			};
			const startHighlighting = () => {
				highlightSession?.dispose();
				const session = openHighlighting(
					handle,
					(list, at) => {
						if (view.current !== created || highlightSession !== session) return;
						let decorations = decorate(list, created.state.doc.length);
						let range = at.range;
						// Tokens for an older version: mapped through the edits since.
						const changes = since(at.version);
						if (!changes) return;
						for (const change of changes) {
							decorations = decorations.map(change);
							if (range)
								range = { from: change.mapPos(range.from, -1), to: change.mapPos(range.to, 1) };
						}
						created.dispatch({ effects: setTokens.of({ decorations, range }) });
					},
					(folds, at) => {
						if (view.current !== created || highlightSession !== session) return;
						const changes = since(at.version);
						if (changes) created.dispatch({ effects: setFolds.of({ folds, changes }) });
					},
				);
				highlightSession = session;
				// A new language: its own tokens and folds, none of the old.
				created.dispatch({
					effects: [setTokens.of({ decorations: Decoration.none }), setFolds.of(null)],
				});
			};
			startHighlighting();
			restartHighlighting.current = startHighlighting;
			symbolsOf.current = async () => {
				// A language server's outline, else the syntax tree's.
				const served = await languageService?.symbols?.().catch(() => null);
				if (served?.length) return served;
				return highlightSession?.symbols ? highlightSession.symbols() : null;
			};
			const startLanguage = (readOnlyNow: boolean) => {
				stopDiagnostics?.();
				stopStatus?.();
				languageService?.dispose?.();
				languageService = openLanguageService({
					projectId: latest.current.projectId,
					document: handle,
					readOnly: readOnlyNow,
					edit: (edits) => {
						if (view.current !== created || !edits.length) return;
						created.dispatch({ changes: edits.map(toChange) });
					},
					indent: () => ({
						tabSize: latest.current.settings.tabSize,
						insertSpaces: !latest.current.settings.useTabs,
					}),
				});
				languageRef.current = languageService;
				stopStatus = languageService?.status?.((status) => {
					if (view.current === created) latest.current.onLanguageStatus?.(status);
				});
				if (!languageService?.status) latest.current.onLanguageStatus?.(null);
				stopDiagnostics = languageService?.diagnostics?.((items) => {
					if (view.current !== created) return;
					created.dispatch({ effects: setProblems.of(items) });
				});
				const tooling = languageExtensions(languageService, readOnlyNow, {
					locations: (list, kind) => latest.current.onLocations?.(list, kind),
					rename: (request) => latest.current.onRename?.(request),
					codeActions: (request) => latest.current.onCodeActions?.(request),
				});
				commands.current = { ...windowCommands, ...tooling.commands };
				created.dispatch({ effects: c.language.reconfigure(tooling.extensions) });
			};
			startLanguage(ro);
			restartLanguage.current = startLanguage;

			return () => {
				restartLanguage.current = undefined;
				restartHighlighting.current = undefined;
				symbolsOf.current = undefined;
				languageRef.current = null;
				stopDiagnostics?.();
				stopStatus?.();
				languageService?.dispose?.();
				highlightSession?.dispose();
				view.current = null;
				created.destroy();
			};
		}, [projectId, path]);

		// Another language chosen: highlighted again (not on the first render).
		const shownLanguage = useRef(language);
		useEffect(() => {
			if (shownLanguage.current === language) return;
			shownLanguage.current = language;
			restartHighlighting.current?.();
		}, [language]);

		// Read-only or editable.
		useEffect(() => {
			const current = view.current;
			if (!current) return;
			current.dispatch({
				effects: compartments.current.readOnly.reconfigure(EditorState.readOnly.of(readOnly)),
			});
			restartLanguage.current?.(readOnly);
		}, [readOnly]);

		// The look, as the settings say.
		useEffect(() => {
			const current = view.current;
			if (!current) return;
			const c = compartments.current;
			current.dispatch({
				effects: [
					c.settings.reconfigure(settingsExtensions(settings, fill)),
					c.lines.reconfigure(
						settings.lineNumbers
							? lineNumbers({ domEventHandlers: { mousedown: gutterSelect } })
							: [],
					),
				],
			});
		}, [settings, fill]);

		// Vim, when asked for: loaded the first time.
		const vimOn = settings.vim;
		useEffect(() => {
			const current = view.current;
			if (!current) return;
			if (!vimOn) {
				current.dispatch({ effects: compartments.current.vim.reconfigure([]) });
				return;
			}
			let cancelled = false;
			let stopFollowing: (() => void) | undefined;
			void loadVim().then((module) => {
				if (cancelled || view.current !== current) return;
				current.dispatch({ effects: compartments.current.vim.reconfigure(module.vim()) });
				stopFollowing = followVim(module, current);
			});
			return () => {
				cancelled = true;
				stopFollowing?.();
			};
		}, [vimOn, projectId, path]);

		// A line asked for: shown, selected and marked a moment.
		const target = props.line ? `${props.jump ?? 0}:${props.line}` : undefined;
		useEffect(() => {
			const current = view.current;
			if (!current || !target) return;
			const number = Number(target.slice(target.indexOf(":") + 1));
			const doc = current.state.doc;
			const line = doc.line(Math.min(Math.max(number, 1), doc.lines));
			current.dispatch({
				selection: EditorSelection.cursor(line.from),
				effects: [EditorView.scrollIntoView(line.from, { y: "center" }), flash.of(line.from)],
			});
			const timer = setTimeout(() => {
				if (view.current === current) current.dispatch({ effects: flash.of(null) });
			}, 1500);
			return () => clearTimeout(timer);
		}, [target]);

		useImperativeHandle(
			ref,
			() => ({
				text: () => (view.current ? textOf(view.current.state, latest.current.lineEnding) : ""),
				replace: (text: string) => {
					const current = view.current;
					if (!current) return;
					const { main } = current.state.selection;
					const clamp = (pos: number) => Math.min(pos, text.length);
					current.dispatch({
						changes: { from: 0, to: current.state.doc.length, insert: text },
						selection: EditorSelection.single(clamp(main.anchor), clamp(main.head)),
						annotations: external.of(true),
					});
				},
				focus: () => view.current?.focus(),
				selections: () =>
					view.current
						? lineSelections(view.current.state.doc, view.current.state.selection.ranges)
						: [],
				symbols: async () => (symbolsOf.current ? symbolsOf.current() : null),
				navigate: (kind) => {
					const current = view.current;
					const service = languageRef.current;
					if (!current || !service) return;
					const ask = service[kind]?.bind(service);
					if (!ask) return;
					void ask(current.state.selection.main.head).then((list) => {
						if (list.length) latest.current.onLocations?.(list, kind);
					});
				},
				offers: (kind) => {
					const service = languageRef.current;
					if (!service) return false;
					switch (kind) {
						case "rename":
							return Boolean(service.rename && service.prepareRename);
						case "codeActions":
							return Boolean(service.codeActions);
						default:
							return Boolean(service[kind]);
					}
				},
				format: async () => {
					const current = view.current;
					const service = languageRef.current;
					if (!current || !service?.format) return false;
					const edits = await service.format();
					if (view.current !== current) return false;
					if (edits.length) current.dispatch({ changes: edits.map(toChange) });
					return true;
				},
				formatOnSave: () => Boolean(languageRef.current?.formatOnSave?.()),
				saved: () => languageRef.current?.saved?.(),
				rename: () => {
					const current = view.current;
					if (current) commands.current["editor.rename"]?.(current);
				},
				codeActions: () => {
					const current = view.current;
					if (current) commands.current["editor.codeActions"]?.(current);
				},
				goToProblem: (direction) => {
					const current = view.current;
					if (!current) return false;
					current.focus();
					return goToProblem(current, direction);
				},
				goToLine: () => {
					const current = view.current;
					if (!current) return;
					current.focus();
					gotoLine(current);
				},
				reveal: (from: number, to: number) => {
					const current = view.current;
					if (!current) return;
					const length = current.state.doc.length;
					const clamp = (pos: number) => Math.max(0, Math.min(pos, length));
					current.dispatch({
						selection: EditorSelection.single(clamp(from), clamp(to)),
						effects: EditorView.scrollIntoView(clamp(from), { y: "center" }),
					});
					current.focus();
				},
			}),
			[],
		);

		return (
			<div ref={box} className={fill ? "relative h-full min-h-0" : "relative"} data-editor="" />
		);
	},
);
