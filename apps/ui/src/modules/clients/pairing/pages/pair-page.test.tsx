import { screen } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { PairPage, safeNext } from "@/modules/clients/pairing/pages/pair-page";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const notPaired = () =>
	http.get("*/api/v1/clients/current", () =>
		HttpResponse.json(
			{ title: "Not authorized", status: 401, code: "not_paired" },
			{ status: 401, headers: { "content-type": "application/problem+json" } },
		),
	);

describe("PairPage", () => {
	it("goes only to paths on this server", () => {
		expect(safeNext("/p/ahab")).toBe("/p/ahab");
		expect(safeNext("//evil.example")).toBe("/");
		expect(safeNext("https://evil.example")).toBe("/");
		expect(safeNext(undefined)).toBe("/");
	});

	it("posts the secret after # and opens where the link says", async () => {
		globalThis.history.replaceState(null, "", "/pair?next=%2Fp%2Fahab#s3cret");
		let sent: unknown;
		mockServer.use(
			notPaired(),
			http.post("*/api/v1/pair", async ({ request }) => {
				sent = await request.json();
				return HttpResponse.json({}, { status: 201 });
			}),
		);
		const go = vi.fn<(path: string) => void>();
		renderWithRouter(<PairPage next="/p/ahab" go={go} />, "/pair");

		await vi.waitFor(() => expect(go).toHaveBeenCalledWith("/p/ahab"));
		expect(sent).toEqual({ secret: "s3cret", kind: "browser" });
		expect(globalThis.location.hash).toBe("");
	});

	it("says when a link no longer works", async () => {
		globalThis.history.replaceState(null, "", "/pair#used");
		mockServer.use(
			notPaired(),
			http.post("*/api/v1/pair", () =>
				HttpResponse.json(
					{ title: "Not allowed", status: 403, code: "pairing_expired" },
					{ status: 403, headers: { "content-type": "application/problem+json" } },
				),
			),
		);
		renderWithRouter(<PairPage go={vi.fn<(path: string) => void>()} />, "/pair");
		expect(await screen.findByText(/used already or expired/)).toBeInTheDocument();
	});
});
