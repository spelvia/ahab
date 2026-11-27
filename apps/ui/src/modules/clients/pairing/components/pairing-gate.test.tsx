import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";

import { PairingGate } from "@/modules/clients/pairing/components/pairing-gate";
import { getCurrentClientMockHandler } from "@/shared/api/generated/clients-client/clients-client.msw";
import { getRequestPairingMockHandler } from "@/shared/api/generated/clients-pairing/clients-pairing.msw";
import { markPaired } from "@/shared/api/pairing";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

describe("PairingGate", () => {
	afterEach(() => {
		markPaired();
		sessionStorage.clear();
	});

	it("shows the app to a paired browser", async () => {
		mockServer.use(getCurrentClientMockHandler());
		renderWithRouter(
			<PairingGate>
				<p>The app</p>
			</PairingGate>,
		);
		expect(await screen.findByText("The app")).toBeInTheDocument();
	});

	it("recovers a failed connection check without asking the browser to pair again", async () => {
		mockServer.use(http.get("*/api/v1/clients/current", () => HttpResponse.error()));
		renderWithRouter(
			<PairingGate>
				<p>The app</p>
			</PairingGate>,
		);
		expect(await screen.findByRole("alert")).toHaveTextContent("Could not reach the Ahab server.");
		mockServer.use(getCurrentClientMockHandler());
		await userEvent.click(screen.getByRole("button", { name: "Retry" }));
		expect(await screen.findByText("The app")).toBeInTheDocument();
	});

	it("shows the pairing screen when the server answers not_paired", async () => {
		mockServer.use(
			http.get("*/api/v1/clients/current", () =>
				HttpResponse.json(
					{ title: "Not authorized", status: 401, code: "not_paired" },
					{ status: 401, headers: { "content-type": "application/problem+json" } },
				),
			),
			getRequestPairingMockHandler({
				id: "r1",
				code: "7F3K",
				secret: "s",
				expiresAt: new Date(Date.now() + 600_000).toISOString(),
			}),
			http.get("*/api/v1/pairing-requests/:id", async () => {
				await delay(5_000);
				return HttpResponse.json({ state: "open" });
			}),
		);
		renderWithRouter(
			<PairingGate>
				<p>The app</p>
			</PairingGate>,
		);
		expect(
			await screen.findByRole("heading", { name: "Your agents, on your machines." }),
		).toBeInTheDocument();
		expect(screen.queryByText("The app")).not.toBeInTheDocument();
	});
});
