import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
	AddCustomAgent,
	AvailableAgents,
} from "@/modules/machines/agents/components/agent-catalog";
import type { AgentCatalog, Machine } from "@/shared/api/generated/ahab.schemas";
import {
	getAddAgentMockHandler,
	getAgentCatalogMockHandler,
} from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { Toaster } from "@/shared/ui/toast";
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

const catalog: AgentCatalog = {
	registryVersion: "1.0.0",
	registrySource: "fetched",
	items: [
		{
			id: "claude-code",
			hasLogo: true,
			monochrome: true,
			name: "Claude Code",
			description: "",
			source: "builtin",
			methods: [],
			addable: false,
		},
		{
			id: "acp:gemini",
			hasLogo: true,
			monochrome: true,
			name: "Gemini CLI",
			description: "Google's agent",
			source: "registry",
			version: "0.68.0",
			methods: ["npx"],
			addable: true,
		},
		{
			id: "acp:amp-acp",
			hasLogo: true,
			monochrome: true,
			name: "Amp",
			description: "Amp over ACP",
			source: "registry",
			version: "0.13.0",
			methods: ["binary"],
			addable: false,
		},
		{
			id: "acp:qwen-code",
			hasLogo: true,
			monochrome: true,
			name: "Qwen Code",
			description: "Alibaba's agent",
			source: "registry",
			methods: ["npx"],
			addable: true,
		},
	],
};

describe("AvailableAgents", () => {
	it("lists the registry's agents not on the machine, adding those it can", async () => {
		const user = userEvent.setup();
		let sent: unknown;
		mockServer.use(
			getAgentCatalogMockHandler(catalog),
			getAddAgentMockHandler(async ({ request }) => {
				sent = await request.json();
				return {
					agent: "acp:gemini",
					name: "Gemini CLI",
					custom: false,
					addedAt: "2026-11-16T10:00:00Z",
				};
			}),
		);
		renderWithQuery(
			<>
				<AvailableAgents
					machine={machine}
					present={new Set(["acp:qwen-code"])}
					query=""
					folded={false}
				/>
				<Toaster />
			</>,
		);

		const section = await screen.findByRole("region", { name: "Available from the ACP registry" });
		expect(within(section).queryByText("Claude Code")).not.toBeInTheDocument();
		expect(within(section).queryByText("Qwen Code")).not.toBeInTheDocument();
		expect(within(section).getByText("Download only")).toBeInTheDocument();
		await user.click(within(section).getByRole("button", { name: "Add" }));

		await expect.poll(() => sent).toEqual({ agent: "acp:gemini", args: [], env: {} });
		expect(await screen.findByText(/Gemini CLI is added to atlas/)).toBeInTheDocument();
	});

	it("finds registry agents by what they are", async () => {
		mockServer.use(getAgentCatalogMockHandler(catalog));
		renderWithQuery(
			<AvailableAgents machine={machine} present={new Set()} query="google" folded={false} />,
		);

		const section = await screen.findByRole("region", { name: "Available from the ACP registry" });
		expect(within(section).getByText("Gemini CLI")).toBeInTheDocument();
		expect(within(section).queryByText("Amp")).not.toBeInTheDocument();
	});
});

describe("AddCustomAgent", () => {
	it("adds any ACP program by name, program and arguments", async () => {
		const user = userEvent.setup();
		let sent: unknown;
		mockServer.use(
			getAddAgentMockHandler(async ({ request }) => {
				sent = await request.json();
				return {
					agent: "custom:my-agent",
					name: "My agent",
					custom: true,
					addedAt: "2026-11-16T10:00:00Z",
				};
			}),
		);
		renderWithQuery(<AddCustomAgent machine={machine} />);

		await user.click(screen.getByRole("button", { name: "Add custom agent" }));
		const dialog = await screen.findByRole("dialog");
		expect(within(dialog).getByRole("button", { name: "Add agent" })).toBeDisabled();
		await user.type(within(dialog).getByLabelText("Name"), "My agent");
		await user.type(within(dialog).getByLabelText("Program"), "my-agent");
		await user.type(within(dialog).getByLabelText("Arguments"), "--acp --verbose");
		await user.click(within(dialog).getByRole("button", { name: "Add agent" }));

		await expect
			.poll(() => sent)
			.toEqual({
				name: "My agent",
				program: "my-agent",
				args: ["--acp", "--verbose"],
				env: {},
			});
	});
});
