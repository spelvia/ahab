/**
 * Problems in the editor, as VS Code shows them: wavy underlines
 * (red errors, amber warnings, blue information, dotted hints), unused code
 * faded and deprecated code struck through; an icon per line in the gutter
 * with its worst problem; marks along the right edge for the whole file;
 * a hover with the message, its tool and rule (linked when it has a page)
 * and the fixes offered; F8 and ⇧F8 to go from one to the next; and, when
 * asked for, each line's worst problem at its end. CodeMirror-specific, so
 * it stays in this folder.
 */
import {
	type EditorState,
	type Extension,
	Facet,
	RangeSetBuilder,
	StateEffect,
	StateField,
} from "@codemirror/state";
import {
	Decoration,
	EditorView,
	gutter,
	GutterMarker,
	hoverTooltip,
	showTooltip,
	type Tooltip,
	ViewPlugin,
	type ViewUpdate,
	WidgetType,
} from "@codemirror/view";

import {
	type CodeAction,
	type Diagnostic,
	type FileEdits,
	nextProblemIndex,
	problemRange,
	type Severity,
	worstSeverity,
} from "@/shared/editor/model";
import { t } from "@/shared/i18n";

/** The file's problems now, from the language service. */
export const setProblems = StateEffect.define<readonly Diagnostic[]>();

/** The problems, kept in step with edits, in order of place. */
const problemsField = StateField.define<readonly Diagnostic[]>({
	create: () => [],
	update(value, transaction) {
		let next = value;
		if (transaction.docChanged)
			next = next.map((problem) => {
				const from = transaction.changes.mapPos(problem.from, 1);
				return { ...problem, from, to: Math.max(from, transaction.changes.mapPos(problem.to, -1)) };
			});
		for (const effect of transaction.effects)
			if (effect.is(setProblems)) {
				const length = transaction.state.doc.length;
				next = effect.value
					.map((problem) =>
						Object.assign({}, problem, {
							from: Math.min(problem.from, length),
							to: Math.min(Math.max(problem.to, problem.from), length),
						}),
					)
					.toSorted((a, b) => a.from - b.from || a.to - b.to);
			}
		return next;
	},
});

/** Whether each line's worst problem shows at its end (Settings → Editor). */
export const problemsAtLineEnd = Facet.define<boolean, boolean>({
	combine: (values) => values.some(Boolean),
});

/** Where a problem is underlined: its range, or the word or line's text at an empty one. */
function underlined(state: EditorState, problem: Diagnostic) {
	const line = state.doc.lineAt(problem.from);
	return problemRange(line, problem.from, Math.min(problem.to, state.doc.length));
}

function markClass(problem: Diagnostic): string {
	const classes = ["cm-ahab-problem", `cm-ahab-problem-${problem.severity}`];
	if (problem.tags?.includes("unnecessary")) {
		classes.push("cm-ahab-unnecessary");
		// Unused code that is only a hint is faded, not underlined, as in VS Code.
		if (problem.severity === "hint") classes.push("cm-ahab-problem-quiet");
	}
	if (problem.tags?.includes("deprecated")) classes.push("cm-ahab-deprecated");
	return classes.join(" ");
}

const marks = EditorView.decorations.compute([problemsField], (state) => {
	const ranges = [];
	for (const problem of state.field(problemsField)) {
		const range = underlined(state, problem);
		if (range.to > range.from)
			ranges.push(Decoration.mark({ class: markClass(problem) }).range(range.from, range.to));
	}
	return Decoration.set(ranges, true);
});

/** Each line's problems, by line start, in order. */
function byLine(state: EditorState): Map<number, Diagnostic[]> {
	const lines = new Map<number, Diagnostic[]>();
	for (const problem of state.field(problemsField)) {
		const start = state.doc.lineAt(problem.from).from;
		const list = lines.get(start);
		if (list) list.push(problem);
		else lines.set(start, [problem]);
	}
	return lines;
}

const RANK: Record<Severity, number> = { error: 0, warning: 1, info: 2, hint: 3 };

/** A line's worst problem: the first of the worst severity. */
function worstOf(list: readonly Diagnostic[]): Diagnostic | undefined {
	return list.toSorted((a, b) => RANK[a.severity] - RANK[b.severity])[0];
}

// Lucide's icons (ISC), drawn in the severity's colour.
const ICONS: Record<Severity, string> = {
	error: '<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>',
	warning:
		'<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
	info: '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
	hint: '<path d="M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5"/><path d="M9 18h6"/><path d="M10 22h4"/>',
};

function icon(severity: Severity): HTMLElement {
	const span = document.createElement("span");
	span.className = `cm-ahab-problem-icon cm-ahab-severity-${severity}`;
	span.setAttribute("aria-label", t(`editor.severity.${severity}`));
	span.innerHTML = `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round">${ICONS[severity]}</svg>`;
	return span;
}

class SeverityMarker extends GutterMarker {
	constructor(readonly severity: Severity) {
		super();
	}
	override eq(other: SeverityMarker) {
		return other.severity === this.severity;
	}
	override toDOM() {
		return icon(this.severity);
	}
}

const MARKERS: Record<Severity, SeverityMarker> = {
	error: new SeverityMarker("error"),
	warning: new SeverityMarker("warning"),
	info: new SeverityMarker("info"),
	hint: new SeverityMarker("hint"),
};

/** The text a line's worst problem shows at its end. */
class LineEndProblem extends WidgetType {
	constructor(
		readonly severity: Severity,
		readonly message: string,
		readonly more: number,
	) {
		super();
	}
	override eq(other: LineEndProblem) {
		return (
			other.severity === this.severity && other.message === this.message && other.more === this.more
		);
	}
	override toDOM() {
		const span = document.createElement("span");
		span.className = `cm-ahab-problem-inline cm-ahab-severity-${this.severity}`;
		const first = this.message.split("\n")[0] ?? "";
		span.textContent = this.more ? `${first} (+${this.more})` : first;
		span.title = this.message;
		return span;
	}
	override ignoreEvent() {
		return false;
	}
}

const lineEnds = EditorView.decorations.compute([problemsField, problemsAtLineEnd], (state) => {
	if (!state.facet(problemsAtLineEnd)) return Decoration.none;
	const builder = new RangeSetBuilder<Decoration>();
	const lines = [...byLine(state)].toSorted((a, b) => a[0] - b[0]);
	for (const [start, list] of lines) {
		const worst = worstOf(list);
		if (!worst) continue;
		const end = state.doc.lineAt(start).to;
		builder.add(
			end,
			end,
			Decoration.widget({
				widget: new LineEndProblem(worst.severity, worst.message, list.length - 1),
				side: 1,
			}),
		);
	}
	return builder.finish();
});

// --- Going to a problem, and its peek ------------------------------------------

const setPeek = StateEffect.define<{ pos: number; problems: readonly Diagnostic[] } | null>();

/** What builds a problem's card: the hover's and F8's. */
type CardMaker = (problems: readonly Diagnostic[], view: EditorView) => HTMLElement;

const cardMaker = Facet.define<CardMaker, CardMaker | undefined>({
	combine: (values) => values[0],
});

/** F8's card under the problem, until the cursor moves or the text changes. */
const peek = StateField.define<Tooltip | null>({
	create: () => null,
	update(value, transaction) {
		let next = value;
		if (transaction.docChanged) next = null;
		const asked = transaction.effects.find((effect) => effect.is(setPeek));
		if (!asked && transaction.selection) next = null;
		if (asked?.is(setPeek)) {
			const shown = asked.value;
			const make = transaction.state.facet(cardMaker);
			next =
				shown && make
					? {
							pos: shown.pos,
							above: false,
							create: (view) => ({ dom: make(shown.problems, view) }),
						}
					: null;
		}
		return next;
	},
	provide: (field) => showTooltip.from(field),
});

/** Goes to the next (1) or previous (-1) problem in the file and shows it; false when there are none. */
export function goToProblem(view: EditorView, direction: 1 | -1): boolean {
	const list = view.state.field(problemsField, false) ?? [];
	const index = nextProblemIndex(list, view.state.selection.main.head, direction);
	const problem = list[index];
	if (!problem) return false;
	const range = underlined(view.state, problem);
	const here = list.filter((each) => each.from === problem.from);
	view.dispatch({
		selection: { anchor: range.from },
		effects: [
			EditorView.scrollIntoView(range.from, { y: "center" }),
			setPeek.of({ pos: range.from, problems: here }),
		],
	});
	return true;
}

/** Shows the problems at a line, as F8 would. */
function showLine(view: EditorView, lineStart: number) {
	const list = byLine(view.state).get(lineStart);
	const first = list?.[0];
	if (!first) return;
	const range = underlined(view.state, first);
	view.dispatch({
		selection: { anchor: range.from },
		effects: [
			EditorView.scrollIntoView(range.from, { y: "center" }),
			setPeek.of({ pos: range.from, problems: list }),
		],
	});
	view.focus();
}

// --- The gutter and the marks along the right edge ------------------------------

const problemGutter = gutter({
	class: "cm-ahab-problem-gutter",
	markers: (view) => {
		const builder = new RangeSetBuilder<GutterMarker>();
		const lines = [...byLine(view.state)].toSorted((a, b) => a[0] - b[0]);
		for (const [start, list] of lines) {
			const worst = worstSeverity(list.map((problem) => problem.severity));
			if (worst) builder.add(start, start, MARKERS[worst]);
		}
		return builder.finish();
	},
	lineMarkerChange: (update) =>
		update.startState.field(problemsField, false) !== update.state.field(problemsField, false),
	initialSpacer: () => MARKERS.error,
	domEventHandlers: {
		mousedown(view, line) {
			if (!byLine(view.state).has(line.from)) return false;
			showLine(view, line.from);
			return true;
		},
	},
});

/** Marks along the right edge: where each line's worst problem is in the whole file. */
const ruler = ViewPlugin.fromClass(
	class {
		readonly dom: HTMLElement;
		constructor(readonly view: EditorView) {
			this.dom = document.createElement("div");
			this.dom.className = "cm-ahab-ruler";
			this.dom.setAttribute("aria-hidden", "true");
			view.dom.append(this.dom);
			this.draw();
		}
		update(update: ViewUpdate) {
			if (
				update.docChanged ||
				update.geometryChanged ||
				update.heightChanged ||
				update.startState.field(problemsField, false) !== update.state.field(problemsField, false)
			)
				this.draw();
		}
		draw() {
			const { view } = this;
			const height = Math.max(view.contentHeight, 1);
			const shown: HTMLElement[] = [];
			const lines = [...byLine(view.state)].toSorted((a, b) => a[0] - b[0]);
			for (const [start, list] of lines) {
				const worst = worstSeverity(list.map((problem) => problem.severity));
				if (!worst || worst === "hint") continue;
				const mark = document.createElement("span");
				mark.className = `cm-ahab-ruler-mark cm-ahab-severity-${worst}`;
				mark.style.top = `${(view.lineBlockAt(start).top / height) * 100}%`;
				mark.title = list.map((problem) => problem.message).join("\n");
				mark.addEventListener("mousedown", (event) => {
					event.preventDefault();
					showLine(view, start);
				});
				shown.push(mark);
			}
			this.dom.replaceChildren(...shown);
		}
		destroy() {
			this.dom.remove();
		}
	},
);

// --- The hover -----------------------------------------------------------------------

/** How fixes are found and applied, when the language service offers them. */
export interface ProblemFixes {
	find: (from: number, to: number) => Promise<CodeAction[]>;
	apply: (edits: FileEdits[]) => Promise<void>;
	/** Read-only: fixes are listed, to apply in Edit mode. */
	readOnly: boolean;
}

/** A problem's card: severity, message, tool and rule (a link when it has a page), and fixes. */
function card(problems: readonly Diagnostic[], fixes: ProblemFixes | undefined): HTMLElement {
	const dom = document.createElement("div");
	dom.className = "cm-ahab-problem-card";
	for (const problem of problems) {
		const row = document.createElement("div");
		row.className = "cm-ahab-problem-row";
		row.append(icon(problem.severity));
		const body = document.createElement("div");
		const message = document.createElement("div");
		message.className = "cm-ahab-problem-message";
		message.textContent = problem.message;
		body.append(message);
		if (problem.source || problem.code) {
			const origin = document.createElement("div");
			origin.className = "cm-ahab-problem-origin";
			origin.append(problem.source ?? "");
			if (problem.code) {
				origin.append("(");
				if (problem.codeHref) {
					const link = document.createElement("a");
					link.href = problem.codeHref;
					link.target = "_blank";
					link.rel = "noreferrer noopener";
					link.textContent = problem.code;
					origin.append(link);
				} else origin.append(problem.code);
				origin.append(")");
			}
			body.append(origin);
		}
		row.append(body);
		dom.append(row);
	}
	const first = problems[0];
	if (fixes && first) {
		const area = document.createElement("div");
		area.className = "cm-ahab-problem-fixes";
		area.textContent = t("editor.fixes.looking");
		dom.append(area);
		const from = Math.min(...problems.map((problem) => problem.from));
		const to = Math.max(...problems.map((problem) => problem.to));
		void fixes
			.find(from, to)
			.then((found) => {
				const quick = found
					.filter((action) => !action.kind || action.kind.startsWith("quickfix"))
					.toSorted((a, b) => Number(Boolean(b.preferred)) - Number(Boolean(a.preferred)))
					.slice(0, 6);
				area.replaceChildren();
				if (!quick.length) {
					area.textContent = t("editor.fixes.none");
					return;
				}
				if (fixes.readOnly) {
					area.textContent = t("editor.fixes.readOnly", { count: quick.length });
					return;
				}
				for (const action of quick) {
					const button = document.createElement("button");
					button.type = "button";
					button.className = "cm-ahab-problem-fix";
					button.textContent = action.title;
					button.addEventListener("click", () => {
						button.disabled = true;
						// Back to the text, so typing and ⌘S go on there.
						const text = button.closest(".cm-editor")?.querySelector<HTMLElement>(".cm-content");
						void action
							.resolve()
							.then((edits) => (edits.length ? fixes.apply(edits) : undefined))
							.then(() => text?.focus())
							.catch(() => {
								button.disabled = false;
							});
					});
					area.append(button);
				}
			})
			.catch(() => {
				area.textContent = t("editor.fixes.none");
			});
	}
	return dom;
}

/** The problems under the pointer, in a card. */
function problemHover(fixes: ProblemFixes | undefined) {
	return hoverTooltip(
		(view, pos) => {
			const here = view.state.field(problemsField).filter((problem) => {
				const range = underlined(view.state, problem);
				return range.from <= pos && pos <= range.to;
			});
			if (!here.length) return null;
			const ranges = here.map((problem) => underlined(view.state, problem));
			return {
				pos: Math.min(...ranges.map((range) => range.from)),
				end: Math.max(...ranges.map((range) => range.to)),
				above: true,
				create: () => ({ dom: card(here, fixes) }),
			};
		},
		{ hoverTime: 300 },
	);
}

const theme = EditorView.theme({
	".cm-ahab-problem": {
		textDecorationLine: "underline",
		textDecorationStyle: "wavy",
		textDecorationSkipInk: "none",
		textUnderlineOffset: "3px",
		textDecorationThickness: "1px",
	},
	".cm-ahab-problem-error": { textDecorationColor: "var(--destructive)" },
	".cm-ahab-problem-warning": { textDecorationColor: "var(--warning)" },
	".cm-ahab-problem-info": { textDecorationColor: "var(--info, #3794ff)" },
	".cm-ahab-problem-hint": {
		textDecorationStyle: "dotted",
		textDecorationThickness: "2px",
		textDecorationColor: "color-mix(in srgb, var(--muted-foreground) 80%, transparent)",
	},
	".cm-ahab-problem-quiet": { textDecorationLine: "none" },
	".cm-ahab-unnecessary": { opacity: "0.6" },
	".cm-ahab-deprecated": { textDecorationLine: "line-through" },
	".cm-ahab-deprecated.cm-ahab-problem:not(.cm-ahab-problem-quiet)": {
		textDecorationLine: "underline line-through",
	},
	".cm-ahab-severity-error": { color: "var(--destructive)" },
	".cm-ahab-severity-warning": { color: "var(--warning)" },
	".cm-ahab-severity-info": { color: "var(--info, #3794ff)" },
	".cm-ahab-severity-hint": { color: "var(--muted-foreground)" },
	".cm-ahab-problem-gutter .cm-gutterElement": {
		display: "flex",
		alignItems: "center",
		justifyContent: "center",
		width: "16px",
		cursor: "pointer",
	},
	".cm-ahab-problem-icon": { display: "inline-flex" },
	".cm-ahab-problem-inline": {
		marginLeft: "2.5em",
		fontStyle: "italic",
		opacity: "0.85",
		whiteSpace: "pre",
		pointerEvents: "none",
	},
	".cm-ahab-ruler": {
		position: "absolute",
		top: "0",
		right: "0",
		bottom: "0",
		width: "8px",
		pointerEvents: "none",
		zIndex: "5",
	},
	".cm-ahab-ruler-mark": {
		position: "absolute",
		left: "0",
		right: "0",
		height: "4px",
		marginTop: "-1px",
		backgroundColor: "currentColor",
		pointerEvents: "auto",
		cursor: "pointer",
	},
	".cm-ahab-problem-card": {
		maxWidth: "36rem",
		maxHeight: "20rem",
		overflow: "auto",
		padding: "6px 8px",
		fontSize: "12px",
		display: "flex",
		flexDirection: "column",
		gap: "6px",
	},
	".cm-ahab-problem-row": { display: "flex", gap: "6px", alignItems: "flex-start" },
	".cm-ahab-problem-row .cm-ahab-problem-icon": { marginTop: "2px" },
	".cm-ahab-problem-message": { whiteSpace: "pre-wrap" },
	".cm-ahab-problem-origin": { color: "var(--muted-foreground)" },
	".cm-ahab-problem-origin a": { color: "var(--primary)", textDecoration: "underline" },
	".cm-ahab-problem-fixes": {
		borderTop: "1px solid var(--border)",
		paddingTop: "4px",
		color: "var(--muted-foreground)",
		display: "flex",
		flexWrap: "wrap",
		gap: "4px",
	},
	".cm-ahab-problem-fix": {
		border: "1px solid var(--input)",
		borderRadius: "4px",
		padding: "1px 6px",
		color: "var(--foreground)",
		backgroundColor: "var(--secondary)",
		cursor: "pointer",
	},
});

/**
 * Everything above, for a document whose language service reports
 * problems; `fixes` when it also offers code actions.
 */
export function problemsExtension(fixes: ProblemFixes | undefined): Extension {
	return [
		problemsField,
		marks,
		lineEnds,
		problemGutter,
		ruler,
		peek,
		cardMaker.of((problems) => card(problems, fixes)),
		problemHover(fixes),
		theme,
	];
}
