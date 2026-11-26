import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { AddClientDialog } from "@/modules/clients/pairing/components/add-client-dialog";
import {
	getCreatePairingMockHandler,
	getGetPairingMockHandler,
	getWithdrawPairingMockHandler,
} from "@/shared/api/generated/clients-pairing/clients-pairing.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const expiresAt = new Date(Date.now() + 600_000).toISOString();

describe("AddClientDialog", () => {
	it("shows a QR code, the link and a code, and closes once the client is paired", async () => {
		let state: "open" | "used" = "open";
		mockServer.use(
			getCreatePairingMockHandler({
				id: "p1",
				url: "http://192.168.1.4:8180/pair#s3cret",
				secret: "s3cret",
				code: "K7QM-4RXT",
				expiresAt,
			}),
			getGetPairingMockHandler(() => ({ id: "p1", state, expiresAt })),
			getWithdrawPairingMockHandler(),
		);
		const onOpenChange = vi.fn<(open: boolean) => void>();
		renderWithQuery(<AddClientDialog open onOpenChange={onOpenChange} />);

		expect(
			await screen.findByRole("img", { name: "QR code of the pairing link" }),
		).toBeInTheDocument();
		expect(screen.getByText("http://192.168.1.4:8180/pair#s3cret")).toBeInTheDocument();
		expect(screen.getByText("K7QM-4RXT")).toBeInTheDocument();
		expect(screen.getByText(/Waiting for the new client/)).toBeInTheDocument();

		state = "used";
		await vi.waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false), { timeout: 5_000 });
	});
});
