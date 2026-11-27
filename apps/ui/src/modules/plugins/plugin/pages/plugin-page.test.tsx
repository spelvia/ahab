import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { PluginPage } from "@/modules/plugins/plugin/pages/plugin-page";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const plugin = {
	id: "docker",
	name: "Docker",
	version: "0.1.0",
	description: "Containers on your machines.",
	source: "folder",
	location: "/home/you/ahab-docker",
	digest: "abc123",
	enabled: false,
	enabledAt: null,
	position: 1,
	entries: ["web", "daemon"],
	hooks: ["terminals.create"],
	secrets: [],
	dependsOn: {},
	placement: "all",
	machines: [],
	manifest: {},
	createdAt: "2026-11-16T10:00:00Z",
	updatedAt: "2026-11-16T10:00:00Z",
};

describe("PluginPage", () => {
	it("shows what a plugin is and may do, and enables it after asking", async () => {
		const enabled: unknown[] = [];
		mockServer.use(
			http.get("*/api/v1/plugins/docker", () => HttpResponse.json(plugin)),
			http.get("*/api/v1/plugins", () => HttpResponse.json({ items: [plugin] })),
			http.get("*/api/v1/plugin-states", () => HttpResponse.json({ items: [] })),
			http.get("*/api/v1/machines", () => HttpResponse.json({ items: [] })),
			http.get("*/api/v1/plugins/docker/files/OVERVIEW.md", () =>
				HttpResponse.text("# Docker\n\nLists **containers**."),
			),
			http.get(
				"*/api/v1/plugins/docker/files/CHANGELOG.md",
				() => new HttpResponse(null, { status: 404 }),
			),
			http.post("*/api/v1/plugins/docker/enable", async ({ request }) => {
				enabled.push(await request.json());
				return HttpResponse.json({ ...plugin, enabled: true });
			}),
		);
		renderWithRouter(<PluginPage id="docker" enable />);
		expect(await screen.findByRole("heading", { name: "Docker", level: 1 })).toBeInTheDocument();
		expect(await screen.findByText("containers")).toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "What it can do" })).toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "Enable Docker?" })).toBeInTheDocument();
		expect(screen.getAllByText("Runs on your machines, as you").length).toBeGreaterThan(0);
		await userEvent.click(screen.getByRole("button", { name: "Enable" }));
		expect(enabled).toEqual([{}]);
		expect(screen.getByText(/has no changelog yet/)).toBeInTheDocument();
	});
});
