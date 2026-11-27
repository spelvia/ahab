import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { WaitingBand } from "@/modules/machines/machine/components/waiting-band";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const waiting: Machine = {
	id: "m1",
	name: "build-box",
	state: "pending",
	isLocal: false,
	online: false,
	checkCode: "K7-4QXM",
	fingerprint: "SHA256:box",
	createdAt: "2026-11-13T09:00:00Z",
};

describe("WaitingBand", () => {
	it("shows each waiting machine's check code and approves it", async () => {
		const user = userEvent.setup();
		let approved = false;
		mockServer.use(
			http.post("*/api/v1/machines/m1/approve", () => {
				approved = true;
				return HttpResponse.json({ ...waiting, state: "active" });
			}),
		);
		renderWithRouter(<WaitingBand machines={[waiting]} />);

		expect(
			await screen.findByRole("heading", { name: "1 machine waits for approval" }),
		).toBeInTheDocument();
		expect(screen.getByText("K7-4QXM")).toBeInTheDocument();
		await user.click(screen.getByRole("button", { name: "Approve build-box" }));

		await expect.poll(() => approved).toBe(true);
	});

	it("asks before rejecting a machine", async () => {
		const user = userEvent.setup();
		renderWithRouter(<WaitingBand machines={[waiting]} />);

		await user.click(await screen.findByRole("button", { name: "Reject" }));

		expect(screen.getByRole("group", { name: "Reject machine" })).toBeInTheDocument();
	});

	it("shows nothing when no machine waits", () => {
		const { container } = renderWithRouter(<WaitingBand machines={[]} />);
		expect(container.querySelector("section")).toBeNull();
	});
});
