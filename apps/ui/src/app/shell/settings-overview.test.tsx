import { screen } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { SettingsLayout } from "@/app/shell/settings-layout";
import { SettingsOverview } from "@/app/shell/settings-overview";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const machine = (name: string, extra: Record<string, unknown> = {}) => ({
	id: `id-${name}`,
	name,
	state: "active",
	online: true,
	outdated: false,
	...extra,
});

describe("Settings", () => {
	it("groups its sections by where they are kept, without listing machines", async () => {
		mockServer.use(
			http.get("*/api/v1/machines", () => HttpResponse.json({ items: [machine("atlas")] })),
		);
		renderWithRouter(<SettingsLayout />, "/settings");
		expect(await screen.findByText("Ahab")).toBeInTheDocument();
		expect(screen.getByText("This browser")).toBeInTheDocument();
		expect(screen.getByRole("link", { name: "Clients" })).toHaveAttribute(
			"href",
			"/settings/clients",
		);
		expect(screen.queryByText("atlas")).not.toBeInTheDocument();
	});

	it("opens on an overview of what needs attention", async () => {
		mockServer.use(
			http.get("*/api/v1/health", () => HttpResponse.json({ status: "ok", version: "0.1.0" })),
			http.get("*/api/v1/machines", () =>
				HttpResponse.json({
					items: [
						machine("atlas"),
						machine("laptop", { outdated: true, online: false }),
						machine("new", { state: "pending", online: false }),
					],
				}),
			),
			http.get("*/api/v1/clients", () =>
				HttpResponse.json({
					items: [
						{ id: "c1", name: "Phone", unused: true },
						{ id: "c2", name: "CLI" },
					],
				}),
			),
			http.get("*/api/v1/projects", () => HttpResponse.json({ items: [] })),
			http.get("*/api/v1/plugins", () => HttpResponse.json({ items: [] })),
		);
		renderWithRouter(<SettingsOverview />);
		expect(await screen.findByText("Version 0.1.0")).toBeInTheDocument();
		expect(await screen.findByText("2 machines")).toBeInTheDocument();
		expect(screen.getByText(/1 behind the server/)).toBeInTheDocument();
		expect(screen.getByText(/1 waiting for approval/)).toBeInTheDocument();
		expect(await screen.findByText("2 clients paired")).toBeInTheDocument();
		expect(screen.getByText(/1 unused for 3 months/)).toBeInTheDocument();
	});
});
