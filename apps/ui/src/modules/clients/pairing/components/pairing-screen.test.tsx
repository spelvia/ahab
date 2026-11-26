import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { PairingScreen } from "@/modules/clients/pairing/components/pairing-screen";
import {
	getPairMockHandler,
	getRequestPairingMockHandler,
} from "@/shared/api/generated/clients-pairing/clients-pairing.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const request = {
	id: "r1",
	code: "7F3K",
	secret: "s3cret",
	expiresAt: new Date(Date.now() + 600_000).toISOString(),
};

describe("PairingScreen", () => {
	beforeEach(() => sessionStorage.clear());

	it("shows the code of its request and waits for approval", async () => {
		mockServer.use(
			getRequestPairingMockHandler(request),
			http.get("*/api/v1/pairing-requests/:id", async () => {
				await delay(5_000);
				return HttpResponse.json({ state: "open" });
			}),
		);
		renderWithQuery(<PairingScreen onPaired={vi.fn<() => void>()} />);

		expect(await screen.findByText("7F3K")).toBeInTheDocument();
		expect(screen.getByText("ahab clients approve 7F3K")).toBeInTheDocument();
		expect(screen.getByText("Waiting for approval…")).toBeInTheDocument();
	});

	it("is paired once a paired client approves", async () => {
		const onPaired = vi.fn<() => void>();
		mockServer.use(
			getRequestPairingMockHandler(request),
			http.get("*/api/v1/pairing-requests/:id", () => HttpResponse.json({ state: "used" })),
		);
		renderWithQuery(<PairingScreen onPaired={onPaired} />);
		await vi.waitFor(() => expect(onPaired).toHaveBeenCalled());
	});

	it("pairs with a code typed from Add client", async () => {
		const onPaired = vi.fn<() => void>();
		let sent: unknown;
		mockServer.use(
			getRequestPairingMockHandler(request),
			http.get("*/api/v1/pairing-requests/:id", async () => {
				await delay(5_000);
				return HttpResponse.json({ state: "open" });
			}),
			getPairMockHandler(async ({ request: posted }) => {
				sent = await posted.json();
				return {
					client: {
						id: "d1",
						name: "Firefox on Linux",
						kind: "browser",
						pairedVia: "code",
						createdAt: "2026-11-17T09:00:00Z",
						encrypted: false,
						current: true,
						unused: false,
					},
				};
			}),
		);
		renderWithQuery(<PairingScreen onPaired={onPaired} />);

		await userEvent.type(await screen.findByPlaceholderText("K7QM-4RXT"), "k7qm-4rxt");
		await userEvent.click(screen.getByRole("button", { name: "Pair" }));
		await vi.waitFor(() => expect(onPaired).toHaveBeenCalled());
		expect(sent).toEqual({ code: "k7qm-4rxt", kind: "browser" });
	});

	it("says why a typed code does not work", async () => {
		mockServer.use(
			getRequestPairingMockHandler(request),
			http.get("*/api/v1/pairing-requests/:id", async () => {
				await delay(5_000);
				return HttpResponse.json({ state: "open" });
			}),
			http.post("*/api/v1/pair", () =>
				HttpResponse.json(
					{ title: "Not allowed", status: 403, code: "pairing_invalid" },
					{ status: 403, headers: { "content-type": "application/problem+json" } },
				),
			),
		);
		renderWithQuery(<PairingScreen onPaired={vi.fn<() => void>()} />);
		await userEvent.type(await screen.findByPlaceholderText("K7QM-4RXT"), "ZZZZ-ZZZZ");
		await userEvent.click(screen.getByRole("button", { name: "Pair" }));
		expect(await screen.findByText(/This code does not work/)).toBeInTheDocument();
	});
});
