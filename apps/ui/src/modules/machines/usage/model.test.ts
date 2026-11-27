import { describe, expect, it } from "vitest";

import { resetsIn, summary, toneOf, worst } from "@/modules/machines/usage/model";
import type { AgentUsage } from "@/shared/api/generated/ahab.schemas";

const usage: AgentUsage = {
	agent: "claude-code",
	label: "Claude Code",
	status: "available",
	plan: "Max 5x",
	fetchedAt: "2026-11-16T16:00:00Z",
	windows: [
		{ id: "session", label: "Session (5 hours)", shortLabel: "5h", usedPercent: 93 },
		{ id: "weekly", label: "Weekly", shortLabel: "wk", usedPercent: 42 },
		{ id: "weekly:fable", label: "Weekly · Fable", shortLabel: null, usedPercent: 0 },
	],
};

describe("usage", () => {
	it("warns past three quarters and alarms past nine tenths", () => {
		expect(toneOf(74)).toBe("ok");
		expect(toneOf(75)).toBe("warning");
		expect(toneOf(90)).toBe("danger");
	});

	it("sums up with the short-named limits, toned by the fullest", () => {
		expect(summary(usage).map((window) => window.id)).toEqual(["session", "weekly"]);
		expect(worst(summary(usage))).toBe("danger");
	});

	it("says when a limit starts over", () => {
		const now = Date.parse("2026-11-16T16:00:00Z");
		expect(resetsIn("2026-11-16T16:53:00Z", now)).toBe("resets in 53 min");
		expect(resetsIn("2026-11-16T20:10:00Z", now)).toBe("resets in 4 h");
		expect(resetsIn("2026-11-16T15:00:00Z", now)).toBe("resets now");
		expect(resetsIn(null, now)).toBe("");
		expect(resetsIn("2026-11-18T12:00:00Z", now)).toMatch(/^resets /);
	});
});
