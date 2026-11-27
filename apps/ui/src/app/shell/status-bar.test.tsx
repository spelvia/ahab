import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { PhoneStatusButton, PhoneStatusSheet, StatusBar } from "@/app/shell/status-bar";
import type { Machine, Project } from "@/shared/api/generated/ahab.schemas";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

// One plugin status item, as a plugin's `status.add` would add it.
vi.mock("@/modules/plugins", () => ({
	useContributions: () => ({ status: [{ key: "p:running", plugin: "p", id: "running" }] }),
	PluginStatusItems: ({ className }: { className?: string }) => (
		<div className={className}>
			<button type="button">3 running</button>
		</div>
	),
}));

const project: Project = {
	id: "p1",
	name: "demo",
	machineId: "m1",
	machineName: "atlas",
	path: "/home/me/demo",
	mode: "readWrite",
	state: "active",
	availability: "available",
	createdAt: "2026-11-25T09:00:00Z",
	updatedAt: "2026-11-25T09:00:00Z",
};

const atlas: Machine = {
	id: "m1",
	name: "atlas",
	state: "active",
	isLocal: true,
	online: true,
	checkCode: "K7-4QXM",
	fingerprint: "SHA256:atlas",
	createdAt: "2026-11-13T09:00:00Z",
};

function serve(counts = { errors: 0, warnings: 0, information: 0, hints: 0 }) {
	mockServer.use(
		http.get("*/api/v1/machines", () => HttpResponse.json({ items: [atlas] })),
		http.get("*/api/v1/projects/p1/problems", () => HttpResponse.json({ counts, items: [] })),
	);
}

const props = {
	project,
	git: false,
	onBranch: () => {},
	fileKey: undefined,
};

describe("StatusBar", () => {
	it("groups the machine, the problems and plugins' items, with no bell and no empty file group", async () => {
		serve();
		renderWithRouter(<StatusBar {...props} onProblems={() => {}} />);
		const bar = await screen.findByRole("contentinfo", { name: "Status bar" });
		const where = within(bar).getByRole("group", { name: "Machine and branch" });
		expect(await within(where).findByRole("link", { name: /^atlas: connected/ })).toBeVisible();
		expect(within(bar).getByRole("button", { name: "3 running" })).toBeVisible();
		expect(within(bar).queryByRole("group", { name: "This file" })).toBeNull();
		expect(within(bar).queryByRole("button", { name: /Notifications/ })).toBeNull();
	});

	it("says no problems quietly, and shows only the counts there are", async () => {
		serve({ errors: 2, warnings: 0, information: 0, hints: 0 });
		const onProblems = vi.fn<() => void>();
		renderWithRouter(<StatusBar {...props} onProblems={onProblems} />);
		const counts = await screen.findByRole("button", {
			name: "2 errors: show or hide the Problems panel",
		});
		expect(counts).toHaveTextContent(/^2$/);
		await userEvent.click(counts);
		expect(onProblems).toHaveBeenCalledOnce();
	});
});

describe("PhoneStatusSheet", () => {
	it("lists the machine, the problems and plugins' items as large rows, and closes for the Problems panel", async () => {
		serve();
		const onOpenChange = vi.fn<(open: boolean) => void>();
		const onProblems = vi.fn<() => void>();
		renderWithRouter(
			<PhoneStatusSheet {...props} open onOpenChange={onOpenChange} onProblems={onProblems} />,
		);
		const sheet = await screen.findByRole("dialog", { name: "Project status" });
		const machine = await within(sheet).findByRole("link", { name: /^atlas: connected/ });
		expect(machine).toHaveTextContent("connected");
		expect(machine).toHaveClass("h-12");
		expect(within(sheet).getByRole("button", { name: "3 running" })).toBeVisible();
		await userEvent.click(
			within(sheet).getByRole("button", { name: "No problems: show or hide the Problems panel" }),
		);
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(onProblems).toHaveBeenCalledOnce();
	});
});

describe("PhoneStatusButton", () => {
	it("shows the machine's connection and the problem count, as a 44px target", async () => {
		serve({ errors: 1, warnings: 2, information: 0, hints: 0 });
		const onOpen = vi.fn<() => void>();
		renderWithRouter(<PhoneStatusButton project={project} onOpen={onOpen} />);
		const button = await screen.findByRole("button", { name: /Project status.*3 problems/ });
		expect(button).toHaveClass("h-11", "min-w-11");
		await userEvent.click(button);
		expect(onOpen).toHaveBeenCalledOnce();
	});
});
