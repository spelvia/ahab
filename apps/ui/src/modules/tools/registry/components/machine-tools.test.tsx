import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { MachineTools } from "@/modules/tools/registry/components/machine-tools";
import type { Machine, MachineToolsDto } from "@/shared/api/generated/ahab.schemas";
import {
	getInstallToolMockHandler,
	getListToolsMockHandler,
} from "@/shared/api/generated/tools-registry/tools-registry.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const machine: Machine = {
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
};

const list: MachineToolsDto = {
	platform: "linux-x64",
	runtimes: [
		{ id: "node", name: "Node", version: "24.18" },
		{ id: "go", name: "Go", version: null },
	],
	tools: [
		{
			id: "rust-analyzer",
			name: "rust-analyzer",
			kind: "languageServer",
			languages: ["rust"],
			origin: "ahab",
			default: true,
			state: "installed",
			foundVersion: "2026-11-10",
			version: "2026-11-16",
			updateAvailable: true,
		},
		{
			id: "basedpyright",
			name: "basedpyright",
			kind: "languageServer",
			languages: ["python"],
			origin: "ahab",
			default: true,
			state: "available",
			version: "1.31.0",
		},
		{
			id: "jdtls",
			name: "jdtls",
			kind: "languageServer",
			languages: ["java"],
			origin: "ahab",
			state: "cannotRun",
			reason: "needs Java 21 or later (found: Java 17)",
		},
		{
			id: "my-linter",
			name: "my-linter",
			kind: "linter",
			languages: ["python"],
			origin: "machine",
			state: "missing",
		},
	],
};

describe("MachineTools", () => {
	it("lists the runtimes and every tool with its state and actions", async () => {
		mockServer.use(getListToolsMockHandler(list));
		renderWithQuery(<MachineTools machine={machine} />);

		const rust = (await screen.findByText("rust-analyzer")).closest("tr")!;
		expect(within(rust).getByText("installed by Ahab")).toBeInTheDocument();
		expect(within(rust).getByText("default")).toBeInTheDocument();
		expect(within(rust).getByText("2026-11-10 → 2026-11-16")).toBeInTheDocument();
		expect(within(rust).getByRole("button", { name: "Update" })).toBeInTheDocument();
		expect(within(rust).getByRole("button", { name: "Remove" })).toBeInTheDocument();
		expect(screen.getByText("needs Java 21 or later (found: Java 17)")).toBeInTheDocument();
		expect(screen.getByText("this machine")).toBeInTheDocument();
		expect(screen.getByText("24.18")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Update all (1)" })).toBeInTheDocument();
	});

	it("filters by state and search", async () => {
		const user = userEvent.setup();
		mockServer.use(getListToolsMockHandler(list));
		renderWithQuery(<MachineTools machine={machine} />);
		await screen.findByText("rust-analyzer");

		await user.selectOptions(screen.getByRole("combobox", { name: "State" }), "problems");
		expect(screen.queryByText("rust-analyzer")).not.toBeInTheDocument();
		expect(screen.getByText("jdtls")).toBeInTheDocument();

		await user.selectOptions(screen.getByRole("combobox", { name: "State" }), "all");
		await user.type(screen.getByRole("textbox", { name: "Search tools" }), "pyright");
		expect(screen.getByText("basedpyright")).toBeInTheDocument();
		expect(screen.queryByText("jdtls")).not.toBeInTheDocument();
	});

	it("shows an install's progress as it starts", async () => {
		const user = userEvent.setup();
		mockServer.use(
			getListToolsMockHandler(list),
			getInstallToolMockHandler({
				action: "install",
				state: "running",
				step: "downloading",
				tool: "basedpyright",
				output: ["Collecting basedpyright==1.31.0"],
			}),
		);
		renderWithQuery(<MachineTools machine={machine} />);

		await user.click(await screen.findByRole("button", { name: "Install" }));
		expect(await screen.findByText("Installing: downloading")).toBeInTheDocument();
		expect(screen.getByText("Collecting basedpyright==1.31.0")).toBeInTheDocument();
	});

	it("asks to update Ahab on a machine too old for language tools", async () => {
		mockServer.use(
			http.get("*/api/v1/machines/:id/tools", () =>
				HttpResponse.json(
					{ type: "about:blank", title: "Conflict", status: 409, code: "machine_outdated" },
					{ status: 409 },
				),
			),
		);
		renderWithQuery(<MachineTools machine={machine} />);
		expect(
			await screen.findByText("Update Ahab on this machine for language tools."),
		).toBeInTheDocument();
	});
});
