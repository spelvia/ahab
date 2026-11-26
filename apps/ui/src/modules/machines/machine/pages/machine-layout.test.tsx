import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { MachineLayout } from "@/modules/machines/machine/pages/machine-layout";
import { MachineOverview } from "@/modules/machines/machine/pages/machine-overview";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import {
	getAgentUpdatesMockHandler,
	getListAgentsMockHandler,
} from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const pending: Machine = {
	id: "m1",
	name: "build-box",
	state: "pending",
	isLocal: false,
	online: false,
	checkCode: "K7-4QXM",
	fingerprint: "SHA256:box",
	createdAt: "2026-11-13T09:00:00Z",
};

describe("MachinePage", () => {
	it("shows the check code of a waiting machine and approves it", async () => {
		const user = userEvent.setup();
		let approved = false;
		mockServer.use(
			http.get("*/api/v1/machines/m1", () =>
				HttpResponse.json(approved ? { ...pending, state: "active" } : pending),
			),
			http.post("*/api/v1/machines/m1/approve", () => {
				approved = true;
				return HttpResponse.json({ ...pending, state: "active" });
			}),
		);
		renderWithRouter(
			<MachineLayout id="m1">
				<p>The open tab</p>
			</MachineLayout>,
		);

		expect(await screen.findByText(/waits for approval\. Approve it only if/)).toBeInTheDocument();
		expect(screen.getAllByText("K7-4QXM").length).toBeGreaterThan(0);
		expect(screen.getByRole("button", { name: "Reject" })).toBeInTheDocument();
		await user.click(screen.getByRole("button", { name: "Approve build-box" }));

		expect(await screen.findAllByText("Offline")).not.toHaveLength(0);
		expect(approved).toBe(true);
	});

	it("shows a machine's tabs and the open one, and its overview", async () => {
		mockServer.use(
			getAgentUpdatesMockHandler({ items: [] }),
			http.get("*/api/v1/machines/m1/metrics", () =>
				HttpResponse.json(
					{ title: "Offline", status: 503, code: "machine_offline" },
					{ status: 503, headers: { "content-type": "application/problem+json" } },
				),
			),
			getListAgentsMockHandler({
				items: [
					{
						agent: "codex",
						label: "Codex",
						state: "notSignedIn",
						checkedAt: new Date().toISOString(),
					},
				],
			}),
			http.get("*/api/v1/machines/m1", () =>
				HttpResponse.json({
					...pending,
					state: "active",
					online: true,
					os: "linux",
					arch: "x86_64",
					version: "0.1.0",
					outdated: true,
					missingMethods: ["machine.metrics"],
				}),
			),
		);
		renderWithRouter(
			<MachineLayout id="m1">
				<MachineOverview id="m1" />
			</MachineLayout>,
		);

		const tabs = await screen.findByRole("navigation", { name: "build-box's tabs" });
		expect(within(tabs).getByRole("link", { name: "Resources" })).toBeInTheDocument();
		expect(within(tabs).getByRole("link", { name: "Agents" })).toHaveAttribute(
			"href",
			"/settings/machines/m1/agents",
		);
		expect(await screen.findByText("Agents ready")).toBeInTheDocument();
		expect(await screen.findByText("0 of 1")).toBeInTheDocument();
		expect(screen.getByText("Agent updates")).toBeInTheDocument();
		expect(await screen.findByText("Every agent is up to date")).toBeInTheDocument();
		// The Resources card, besides the tab of that name.
		expect(screen.getAllByText("Resources").length).toBeGreaterThan(1);
		expect(screen.queryByText("Needs you")).not.toBeInTheDocument();
		expect(screen.queryByText("Projects")).not.toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "System" })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: /^Approve/ })).not.toBeInTheDocument();
		expect(screen.getByText("Older Ahab")).toHaveAttribute(
			"title",
			"Update Ahab on this machine to use: machine.metrics",
		);
	});

	it("explains a machine that is gone", async () => {
		mockServer.use(
			http.get("*/api/v1/machines/m1", () =>
				HttpResponse.json(
					{ title: "Not found", status: 404, code: "machine_not_found" },
					{ status: 404, headers: { "content-type": "application/problem+json" } },
				),
			),
		);
		renderWithRouter(
			<MachineLayout id="m1">
				<p>The open tab</p>
			</MachineLayout>,
		);

		expect(await screen.findByText(/its request expired/)).toBeInTheDocument();
	});
});
