import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { UsagePanel, usePlanUsage } from "@/modules/machines/usage/components/usage-panel";
import type { AgentUsage, AgentUsageList } from "@/shared/api/generated/ahab.schemas";
import { getAgentUsageMockHandler } from "@/shared/api/generated/machines-usage/machines-usage.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

/** The hook's answer, as text. */
function Probe({ agent }: { agent: string }) {
	const plan = usePlanUsage("m1", agent);
	return <p>{plan ? `tone ${plan.tone}` : "none"}</p>;
}

const claude: AgentUsage = {
	agent: "claude-code",
	label: "Claude Code",
	status: "available",
	plan: "Max 5x",
	fetchedAt: new Date().toISOString(),
	windows: [
		{ id: "session", label: "Session (5 hours)", shortLabel: "5h", usedPercent: 93 },
		{ id: "weekly", label: "Weekly", shortLabel: "wk", usedPercent: 42 },
		{ id: "weekly:opus", label: "Weekly · Opus", shortLabel: null, usedPercent: 10 },
	],
};
const list: AgentUsageList = {
	items: [
		claude,
		{
			agent: "codex",
			label: "Codex",
			status: "unavailable",
			problem: "Not signed in with a ChatGPT plan here.",
			fetchedAt: new Date().toISOString(),
			windows: [],
		},
	],
};

describe("plan usage", () => {
	it("is read for metered agents with readable limits, with its nearest limit's tone", async () => {
		mockServer.use(getAgentUsageMockHandler(list));
		renderWithQuery(<Probe agent="claude-code" />);
		expect(await screen.findByText("tone danger")).toBeInTheDocument();
		renderWithQuery(<Probe agent="codex" />);
		await new Promise((resolve) => setTimeout(resolve, 50));
		expect(screen.getByText("none")).toBeInTheDocument();
	});

	it("never asks for agents Ahab cannot read limits of", async () => {
		let asked = false;
		mockServer.use(
			getAgentUsageMockHandler(() => {
				asked = true;
				return list;
			}),
		);
		renderWithQuery(<Probe agent="pi" />);
		await new Promise((resolve) => setTimeout(resolve, 50));
		expect(asked).toBe(false);
	});

	it("shows every limit and the plan", () => {
		renderWithQuery(<UsagePanel machineId="m1" usage={claude} />);
		expect(screen.getByText("Weekly · Opus")).toBeInTheDocument();
		expect(screen.getByText(/· Max 5x/)).toBeInTheDocument();
		expect(screen.getAllByRole("meter")).toHaveLength(3);
	});
});
