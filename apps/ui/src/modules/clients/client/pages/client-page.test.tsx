import { screen } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { ClientPage } from "@/modules/clients/client/pages/client-page";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const client = (id: string, name: string, extra: Record<string, unknown> = {}) => ({
	id,
	name,
	kind: "browser",
	current: false,
	encrypted: true,
	unused: false,
	pairedVia: "code",
	createdAt: "2026-11-20T10:00:00Z",
	lastUsedAt: null,
	...extra,
});

describe("ClientPage", () => {
	it("shows how a client was paired, by whom, and its actions", async () => {
		mockServer.use(
			http.get("*/api/v1/clients", () =>
				HttpResponse.json({
					items: [
						client("laptop", "Laptop", { current: true }),
						client("phone", "Phone", {
							kind: "app",
							pairedBy: "laptop",
							encrypted: false,
							unused: true,
						}),
					],
				}),
			),
		);
		renderWithRouter(<ClientPage clientId="phone" />);
		expect(await screen.findByRole("heading", { level: 1, name: "Phone" })).toBeInTheDocument();
		expect(screen.getByRole("link", { name: "Clients" })).toHaveAttribute(
			"href",
			"/settings/clients",
		);
		expect(screen.getByText("Not encrypted")).toBeInTheDocument();
		expect(screen.getByRole("link", { name: "by Laptop" })).toHaveAttribute(
			"href",
			"/settings/clients/laptop",
		);
		expect(screen.getByRole("button", { name: "Actions for Phone" })).toBeInTheDocument();
	});

	it("says so when the client is gone", async () => {
		mockServer.use(http.get("*/api/v1/clients", () => HttpResponse.json({ items: [] })));
		renderWithRouter(<ClientPage clientId="gone" />);
		expect(await screen.findByText("This client is not paired any more.")).toBeInTheDocument();
	});
});
