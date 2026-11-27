import { describe, expect, it } from "vitest";

import {
	DEFAULT_HEIGHT,
	linesSince,
	MIN_HEIGHT,
	nextPanel,
	panelState,
	savePanel,
	shownHeight,
} from "@/app/shell/panel-state";

describe("the bottom panel's state", () => {
	it("is closed on Problems until asked, and kept per project", () => {
		expect(panelState({}, "w1")).toEqual({
			open: false,
			tab: "problems",
			height: DEFAULT_HEIGHT,
			maximised: false,
		});
		const saved = savePanel({}, "w1", { ...panelState({}, "w1"), open: true, tab: "output" });
		expect(panelState(saved, "w1")).toMatchObject({ open: true, tab: "output" });
		expect(panelState(saved, "w2")).toMatchObject({ open: false, tab: "problems" });
		expect(panelState({ tab: "debug", height: 12 }, "w1")).toMatchObject({
			tab: "problems",
			height: MIN_HEIGHT,
		});
		const closed = savePanel(saved, "w1", { ...panelState(saved, "w1"), open: false });
		expect(closed.open).toEqual({});
	});

	it("toggles as VS Code's does", () => {
		const closed = panelState({}, "w1");
		const problems = nextPanel(closed, { kind: "toggleTab", tab: "problems" });
		expect(problems).toMatchObject({ open: true, tab: "problems" });
		expect(nextPanel(problems, { kind: "toggleTab", tab: "problems" }).open).toBe(false);
		const output = nextPanel(problems, { kind: "toggleTab", tab: "output" });
		expect(output).toMatchObject({ open: true, tab: "output" });
		expect(nextPanel(output, { kind: "toggle" })).toMatchObject({ open: false, tab: "output" });
		expect(nextPanel(closed, { kind: "toggle" })).toMatchObject({ open: true, tab: "problems" });
		expect(nextPanel(output, { kind: "show", tab: "output" })).toMatchObject({ open: true });
		const big = nextPanel(problems, { kind: "maximise", on: true });
		expect(nextPanel(big, { kind: "close" })).toMatchObject({ open: false, maximised: false });
		expect(nextPanel(problems, { kind: "resize", height: 10 }).height).toBe(MIN_HEIGHT);
	});

	it("leaves the windows above some room", () => {
		expect(shownHeight(240, 800)).toBe(240);
		expect(shownHeight(900, 800)).toBe(680);
		expect(shownHeight(240, 100)).toBe(100);
	});

	it("shows a log's lines since it was cleared", () => {
		const lines = ["a", "b", "c"];
		expect(linesSince(lines, undefined)).toEqual(lines);
		expect(linesSince(lines, { line: "c", count: 3 })).toEqual([]);
		expect(linesSince([...lines, "d"], { line: "c", count: 3 })).toEqual(["d"]);
		// The log dropped its first lines meanwhile.
		expect(linesSince(["b", "c", "d", "e"], { line: "c", count: 3 })).toEqual(["d", "e"]);
		// Restarted: the mark is gone.
		expect(linesSince(["x"], { line: "c", count: 3 })).toEqual(["x"]);
	});
});
