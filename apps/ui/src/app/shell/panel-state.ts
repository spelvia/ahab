/**
 * The bottom panel's state (Problems, Output), as VS Code's panel behaves:
 * one panel under the windows, with its view kept per project.
 */
import type { BottomPanel } from "@/shared/preferences/store";

export type PanelTab = "problems" | "output";

const PANEL_TABS: readonly PanelTab[] = ["problems", "output"];

export const DEFAULT_HEIGHT = 240;
export const MIN_HEIGHT = 120;
/** The windows above keep at least this much. */
const KEEP_ABOVE = 120;

export interface PanelState {
	open: boolean;
	tab: PanelTab;
	height: number;
	maximised: boolean;
}

/** The panel as kept for a project; closed, on Problems, by default. */
export function panelState(saved: BottomPanel, projectId: string | undefined): PanelState {
	const view = (projectId ? saved.views?.[projectId] : undefined) ?? saved;
	const tab = PANEL_TABS.find((each) => each === view.tab) ?? "problems";
	const height =
		typeof view.height === "number" && Number.isFinite(view.height)
			? Math.max(MIN_HEIGHT, Math.round(view.height))
			: DEFAULT_HEIGHT;
	return {
		open: Boolean(projectId && saved.open?.[projectId]),
		tab,
		height,
		maximised: Boolean(view.maximised),
	};
}

/** What a request does to the panel. */
export type PanelRequest =
	/** ⌘J: open or close it, on the tab it had. */
	| { kind: "toggle" }
	/** The counts in the status bar: open on a tab, or close it when it shows that tab. */
	| { kind: "toggleTab"; tab: PanelTab }
	/** Focus problems, show output: open on a tab. */
	| { kind: "show"; tab: PanelTab }
	| { kind: "close" }
	| { kind: "maximise"; on: boolean }
	| { kind: "resize"; height: number };

export function nextPanel(state: PanelState, request: PanelRequest): PanelState {
	switch (request.kind) {
		case "toggle":
			return { ...state, open: !state.open };
		case "toggleTab":
			return state.open && state.tab === request.tab
				? { ...state, open: false }
				: { ...state, open: true, tab: request.tab };
		case "show":
			return { ...state, open: true, tab: request.tab };
		case "close":
			return { ...state, open: false, maximised: false };
		case "maximise":
			return { ...state, open: true, maximised: request.on };
		case "resize":
			return { ...state, height: Math.max(MIN_HEIGHT, Math.round(request.height)) };
	}
}

/** The state kept again, for a project. */
export function savePanel(
	saved: BottomPanel,
	projectId: string | undefined,
	state: PanelState,
): BottomPanel {
	if (!projectId) return saved;
	const open = { ...saved.open };
	if (state.open) open[projectId] = true;
	else delete open[projectId];
	const { tab, height, maximised } = state;
	return { ...saved, open, views: { ...saved.views, [projectId]: { tab, height, maximised } } };
}

/** The height to show in a space `available` tall: the windows above keep some room. */
export function shownHeight(height: number, available: number): number {
	if (available <= 0) return height;
	return Math.max(Math.min(MIN_HEIGHT, available), Math.min(height, available - KEEP_ABOVE));
}

/**
 * A log's lines after it was cleared: the lines after the last one shown
 * then (`mark`: that line and how many lines there were). Logs keep their
 * last lines only, so the mark is looked for from the end; when it is gone
 * (the log moved past it, or restarted), every line shows.
 */
export function linesSince(
	lines: readonly string[],
	mark: { line: string; count: number } | undefined,
): readonly string[] {
	if (!mark) return lines;
	// Unchanged since: nothing new.
	if (lines.length === mark.count && lines.at(-1) === mark.line) return [];
	for (let index = lines.length - 1; index >= 0; index--)
		if (lines[index] === mark.line) return lines.slice(index + 1);
	return lines;
}
