import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { AgentUpdateLine } from "@/modules/machines/agents/components/agent-update";
import type { AgentUpdate, Machine } from "@/shared/api/generated/ahab.schemas";
import { getUpdateAgentMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const machine: Machine = {
	id: "m1",
	name: "atlas",
	state: "active",
	isLocal: true,
	online: true,
	checkCode: "K7",
	fingerprint: "SHA256:x",
	createdAt: "2026-11-16T10:00:00Z",
};

const codex: AgentUpdate = {
	agent: "codex",
	label: "Codex",
	installed: "0.167.1",
	latest: "0.169.0",
	channel: "latest",
	available: true,
	method: "npm",
	canUpdate: true,
	command: "codex update",
	checkedAt: "2026-11-16T10:00:00Z",
};

describe("AgentUpdateLine", () => {
	it("updates an agent after asking, and says what happened", async () => {
		const user = userEvent.setup();
		let asked = 0;
		mockServer.use(
			getUpdateAgentMockHandler(() => {
				asked += 1;
				return {
					agent: "codex",
					before: "0.167.1",
					after: "0.169.0",
					success: true,
					output: "installed",
				};
			}),
		);
		renderWithQuery(<AgentUpdateLine machine={machine} update={codex} />);

		expect(screen.getByText(/is out/)).toHaveTextContent("0.169.0 is out (0.167.1 here)");
		await user.click(screen.getByRole("button", { name: "Update to 0.169.0" }));
		expect(await screen.findByText(/Sessions using Codex on atlas may stop/)).toBeInTheDocument();
		expect(asked).toBe(0);
		await user.click(screen.getByRole("button", { name: "Update" }));

		expect(await screen.findByText("Updated from 0.167.1 to 0.169.0.")).toBeInTheDocument();
		expect(asked).toBe(1);
	});

	it("says how to update an agent Nix installed, without a button", () => {
		renderWithQuery(
			<AgentUpdateLine
				machine={machine}
				update={{
					...codex,
					method: "nix",
					canUpdate: false,
					reason: "installed by Nix: the system's configuration updates it",
					command: "nix flake update, then rebuild",
				}}
			/>,
		);

		expect(screen.queryByRole("button")).not.toBeInTheDocument();
		expect(screen.getByText(/installed by Nix/)).toHaveTextContent(
			"nix flake update, then rebuild",
		);
	});

	it("says when an agent is up to date", () => {
		renderWithQuery(
			<AgentUpdateLine
				machine={machine}
				update={{ ...codex, installed: "0.169.0", available: false }}
			/>,
		);
		expect(screen.getByText(/Up to date with/)).toHaveTextContent(
			"Up to date with 0.169.0 on latest",
		);
	});
});
