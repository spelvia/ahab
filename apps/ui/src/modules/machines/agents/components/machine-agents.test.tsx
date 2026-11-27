import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { MachineAgents } from "@/modules/machines/agents/components/machine-agents";
import type { AgentStatus, Machine } from "@/shared/api/generated/ahab.schemas";
import {
	getAgentUpdatesMockHandler,
	getListAgentsMockHandler,
	getRefreshAgentsMockHandler,
} from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { getAgentUsageMockHandler } from "@/shared/api/generated/machines-usage/machines-usage.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

function machine(overrides: Partial<Machine> = {}): Machine {
	return {
		id: "0193-local",
		name: "atlas",
		state: "active",
		isLocal: true,
		online: true,
		checkCode: "K7-4QXM",
		fingerprint: "SHA256:local",
		os: "linux",
		arch: "x86_64",
		version: "0.1.0",
		createdAt: "2026-11-13T09:00:00Z",
		approvedAt: "2026-11-13T09:00:00Z",
		lastSeenAt: new Date().toISOString(),
		...overrides,
	};
}

const buildBox = machine({
	id: "0193-box",
	name: "build-box",
	isLocal: false,
	online: false,
	fingerprint: "SHA256:box",
	lastSeenAt: null,
});

function agent(overrides: Partial<AgentStatus> = {}): AgentStatus {
	return {
		agent: "claude-code",
		label: "Claude Code",
		state: "ready",
		version: "2.1.322",
		account: "me@example.com",
		checkedAt: new Date().toISOString(),
		...overrides,
	};
}

const agents = [
	agent(),
	agent({ agent: "codex", label: "Codex", state: "notSignedIn", account: null }),
	agent({
		agent: "pi",
		label: "Pi",
		state: "notSignedIn",
		account: null,
		detail: "Pi reports its default provider as `invalid`",
	}),
	agent({ agent: "opencode", label: "OpenCode", state: "notInstalled", version: null }),
];

function Harness({ machine: item }: { machine: Machine }) {
	const [selected, setSelected] = useState<string>();
	return <MachineAgents machine={item} selected={selected} onSelect={setSelected} />;
}

async function render(item: Machine, items: AgentStatus[] = agents) {
	mockServer.use(
		getListAgentsMockHandler({ items }),
		getAgentUpdatesMockHandler({
			items: [
				{
					agent: "claude-code",
					label: "Claude Code",
					installed: "2.1.322",
					latest: "2.1.325",
					channel: "latest",
					available: true,
					method: "native",
					canUpdate: true,
					command: "claude update",
					checkedAt: new Date().toISOString(),
				},
			],
		}),
		getAgentUsageMockHandler({
			items: [
				{
					agent: "claude-code",
					label: "Claude Code",
					status: "available",
					plan: "Max 5x",
					fetchedAt: new Date().toISOString(),
					windows: [
						{ id: "session", label: "Session (5 hours)", shortLabel: "5h", usedPercent: 42 },
					],
				},
			],
		}),
	);
	renderWithRouter(<Harness machine={item} />);
	await screen.findByText(/Checked|Not checked/);
}

describe("MachineAgents", () => {
	it("puts what needs you first, then the installed, and folds the rest", async () => {
		const user = userEvent.setup();
		await render(machine());

		const attention = await screen.findByRole("region", { name: "Needs you" });
		expect(within(attention).getByRole("button", { name: "Codex: details" })).toBeInTheDocument();
		expect(within(attention).getByRole("button", { name: "Pi: details" })).toBeInTheDocument();
		const ready = screen.getByRole("region", { name: "Installed" });
		expect(within(ready).getByRole("button", { name: "Claude Code: details" })).toBeInTheDocument();
		expect(await within(ready).findByRole("button", { name: "Update" })).toBeInTheDocument();
		expect(within(ready).getByText("42%")).toBeInTheDocument();

		const missing = screen.getByRole("region", { name: "Not installed here" });
		expect(within(missing).queryByText("OpenCode")).not.toBeInTheDocument();
		await user.click(within(missing).getByRole("button", { name: /Not installed here/ }));
		expect(within(missing).getByText("OpenCode")).toBeInTheDocument();
	});

	it("finds agents by name and filters them by state", async () => {
		const user = userEvent.setup();
		await render(machine());

		await user.type(await screen.findByRole("textbox", { name: "Search agents" }), "cod");
		expect(screen.getByRole("button", { name: "Codex: details" })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Pi: details" })).not.toBeInTheDocument();

		await user.clear(screen.getByRole("textbox", { name: "Search agents" }));
		await user.click(await screen.findByRole("button", { name: "Update 1" }));
		expect(screen.getByRole("button", { name: "Claude Code: details" })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Codex: details" })).not.toBeInTheDocument();
	});

	it("opens an agent's details in a drawer", async () => {
		const user = userEvent.setup();
		await render(machine());

		await user.click(await screen.findByRole("button", { name: "Claude Code: details" }));
		const drawer = await screen.findByRole("dialog", { name: "Claude Code" });
		expect(within(drawer).getByText("me@example.com · Max 5x")).toBeInTheDocument();
		expect(within(drawer).getByRole("meter", { name: "Session (5 hours)" })).toBeInTheDocument();
		expect(await within(drawer).findByText(/is out/)).toHaveTextContent("2.1.325 is out");

		await user.keyboard("{Escape}");
		await user.click(screen.getByRole("button", { name: "Codex: details" }));
		expect(
			await screen.findByText(
				(_, node) => node?.textContent === "Run codex login on atlas to sign in.",
			),
		).toBeInTheDocument();
	});

	it("checks the agents again on request", async () => {
		const user = userEvent.setup();
		mockServer.use(getRefreshAgentsMockHandler({ items: [agent({ state: "notSignedIn" })] }));
		await render(machine());

		await user.click(await screen.findByRole("button", { name: "Check again" }));
		const attention = await screen.findByRole("region", { name: "Needs you" });
		expect(
			await within(attention).findByRole("button", { name: "Claude Code: details" }),
		).toBeInTheDocument();
	});

	it("cannot check an offline machine again", async () => {
		await render(buildBox);

		expect(screen.getByRole("button", { name: "Check again" })).toBeDisabled();
	});
});
