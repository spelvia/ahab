import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { ClientsPage } from "@/modules/clients/client/pages/clients-page";
import type { Client } from "@/shared/api/generated/ahab.schemas";
import { getListClientsMockHandler } from "@/shared/api/generated/clients-client/clients-client.msw";
import {
	getApprovePairingRequestMockHandler,
	getListPairingRequestsMockHandler,
} from "@/shared/api/generated/clients-pairing/clients-pairing.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const client = (overrides: Partial<Client>): Client => ({
	id: "d",
	name: "Client",
	kind: "browser",
	pairedVia: "link",
	createdAt: "2026-08-01T09:00:00Z",
	lastUsedAt: "2026-11-17T09:00:00Z",
	encrypted: true,
	current: false,
	unused: false,
	...overrides,
});

const request = {
	id: "r1",
	code: "7F3K",
	clientName: "ahab CLI on build-box",
	clientKind: "cli" as const,
	address: "192.168.178.40",
	state: "open" as const,
	expiresAt: new Date(Date.now() + 600_000).toISOString(),
	createdAt: new Date().toISOString(),
};

const row = (name: string) => screen.getByText(name).closest("tr") as HTMLElement;

describe("ClientsPage", () => {
	it("lists clients with what to know about each", async () => {
		mockServer.use(
			getListPairingRequestsMockHandler({ items: [] }),
			getListClientsMockHandler({
				items: [
					client({ id: "a", name: "This computer", kind: "local", current: false }),
					client({ id: "b", name: "Firefox on Linux", current: true, pairedBy: "a" }),
					client({ id: "c", name: "Safari on iPhone", encrypted: false }),
					client({ id: "d", name: "Old tablet", unused: true }),
				],
			}),
		);
		renderWithRouter(<ClientsPage />);

		expect(await screen.findByText("4 paired")).toBeInTheDocument();
		expect(within(row("Firefox on Linux")).getByText("This client")).toBeInTheDocument();
		expect(within(row("Firefox on Linux")).getByText("by This computer")).toBeInTheDocument();
		expect(within(row("Safari on iPhone")).getByText("Not encrypted")).toBeInTheDocument();
		expect(within(row("Old tablet")).getByText("Not used for 3 months")).toBeInTheDocument();
		expect(
			screen.getByText("1 client hasn't been used for 3 months. Revoke it?"),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Revoke all unused" })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Add client/ })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Create token/ })).toBeInTheDocument();
	});

	it("shows clients asking to be paired, to approve", async () => {
		let approved = "";
		mockServer.use(
			getListClientsMockHandler({ items: [client({ current: true })] }),
			getListPairingRequestsMockHandler({ items: [request] }),
			getApprovePairingRequestMockHandler(({ params }) => {
				approved = params.id;
				return { ...request, state: "approved" };
			}),
		);
		renderWithRouter(<ClientsPage approve="7f3k" />);

		const prompt = await screen.findByRole("region", {
			name: "ahab CLI on build-box wants to pair",
		});
		expect(within(prompt).getByText("7F3K")).toBeInTheDocument();
		expect(within(prompt).getByText("· 192.168.178.40")).toBeInTheDocument();
		// Opened for this request (its QR code or link): marked, and asked to compare.
		expect(
			within(prompt).getByText(/Opened from this client's link or QR code/),
		).toBeInTheDocument();
		await userEvent.click(within(prompt).getByRole("button", { name: "Approve" }));
		await screen.findByText("1 client asks to be paired");
		expect(approved).toBe("r1");
	});
});
