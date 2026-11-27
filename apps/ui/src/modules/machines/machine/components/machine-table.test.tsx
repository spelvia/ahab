import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { MachineTable } from "@/modules/machines/machine/components/machine-table";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { getListAgentsMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { Toaster } from "@/shared/ui/toast";
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

async function render(machines: Machine[]) {
	mockServer.use(
		getListAgentsMockHandler({
			items: [
				{
					agent: "claude-code",
					label: "Claude Code",
					state: "ready",
					checkedAt: new Date().toISOString(),
				},
				{ agent: "pi", label: "Pi", state: "notInstalled", checkedAt: new Date().toISOString() },
			],
		}),
	);
	renderWithRouter(
		<>
			<MachineTable machines={machines} />
			<Toaster />
		</>,
	);
	await screen.findByRole("table");
}

describe("MachineTable", () => {
	it("lists each machine with its status, platform, version and last seen", async () => {
		await render([machine(), buildBox]);

		const [, local, other] = screen.getAllByRole("row") as [HTMLElement, HTMLElement, HTMLElement];
		expect(within(local).getByRole("link", { name: "atlas" })).toBeInTheDocument();
		expect(within(local).getByText("This server")).toBeInTheDocument();
		expect(within(local).getByText("Online")).toBeInTheDocument();
		expect(within(local).getByText("Connected")).toBeInTheDocument();
		expect(within(other).getByText("Offline")).toBeInTheDocument();
		expect(within(other).getByText("Never")).toBeInTheDocument();
	});

	it("shows the agents each machine can run, leaving out those not installed", async () => {
		await render([machine()]);

		expect(await screen.findByText("Claude Code: Ready")).toBeInTheDocument();
		expect(screen.queryByText(/^Pi:/)).not.toBeInTheDocument();
	});

	it("keeps the server's own machine out of reach of Remove", async () => {
		const user = userEvent.setup();
		await render([machine()]);

		await user.click(screen.getByRole("button", { name: "Actions for atlas" }));

		expect(await screen.findByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
		expect(screen.queryByRole("menuitem", { name: "Remove" })).not.toBeInTheDocument();
	});

	it("renames a machine from its menu", async () => {
		const user = userEvent.setup();
		let sent: unknown;
		mockServer.use(
			http.patch("*/api/v1/machines/:id", async ({ request }) => {
				sent = await request.json();
				return HttpResponse.json(machine({ name: "workstation" }));
			}),
		);
		await render([machine()]);

		await user.click(screen.getByRole("button", { name: "Actions for atlas" }));
		await user.click(await screen.findByRole("menuitem", { name: "Rename" }));
		const input = screen.getByLabelText("Name");
		await user.clear(input);
		await user.type(input, "workstation");
		await user.click(screen.getByRole("button", { name: "Save" }));

		await expect.poll(() => sent).toEqual({ name: "workstation" });
	});

	it("explains an invalid name before sending it", async () => {
		const user = userEvent.setup();
		await render([machine()]);

		await user.click(screen.getByRole("button", { name: "Actions for atlas" }));
		await user.click(await screen.findByRole("menuitem", { name: "Rename" }));
		const input = screen.getByLabelText("Name");
		await user.clear(input);
		await user.type(input, "Build Box");

		expect(screen.getByRole("alert")).toHaveTextContent(
			"Use lowercase letters, digits and hyphens.",
		);
		expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
	});

	it("asks before removing a machine", async () => {
		const user = userEvent.setup();
		let removed = false;
		mockServer.use(
			http.delete("*/api/v1/machines/:id", () => {
				removed = true;
				return new HttpResponse(null, { status: 204 });
			}),
		);
		await render([buildBox]);

		await user.click(screen.getByRole("button", { name: "Actions for build-box" }));
		await user.click(await screen.findByRole("menuitem", { name: "Remove" }));
		expect(screen.getByText(/It must join again to come back/)).toBeInTheDocument();
		await user.click(screen.getByRole("button", { name: "Remove" }));

		await expect.poll(() => removed).toBe(true);
	});

	it("copies a fingerprint and says so", async () => {
		const user = userEvent.setup();
		await render([machine()]);

		await user.click(screen.getByRole("button", { name: "Actions for atlas" }));
		await user.click(await screen.findByRole("menuitem", { name: "Copy fingerprint" }));

		expect(await screen.findByRole("status")).toHaveTextContent("Copied atlas's fingerprint.");
		await expect.poll(() => navigator.clipboard.readText()).toBe("SHA256:local");
	});
});
