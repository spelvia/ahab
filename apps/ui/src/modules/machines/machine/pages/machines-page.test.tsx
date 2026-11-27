import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { MachinesPage } from "@/modules/machines/machine/pages/machines-page";
import { getListAgentsMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { getListMachinesMockHandler } from "@/shared/api/generated/machines-machine/machines-machine.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const base = {
	isLocal: false,
	fingerprint: "SHA256:x",
	checkCode: "K7-4QXM",
	createdAt: "2026-11-13T09:00:00Z",
};

describe("MachinesPage", () => {
	it("explains how to add the first machine", async () => {
		mockServer.use(getListMachinesMockHandler({ items: [] }));

		renderWithRouter(<MachinesPage />);

		expect(await screen.findByText("No machines yet")).toBeInTheDocument();
		expect(screen.getByText("ahab up --daemon=true")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Add machine/ })).toBeInTheDocument();
	});

	it("puts waiting machines above the table and counts the online ones", async () => {
		mockServer.use(
			getListAgentsMockHandler({ items: [] }),
			getListMachinesMockHandler({
				items: [
					{ ...base, id: "a", name: "atlas", state: "active", online: true, isLocal: true },
					{ ...base, id: "b", name: "laptop", state: "active", online: false },
					{ ...base, id: "c", name: "build-box", state: "pending", online: false },
				],
			}),
		);

		renderWithRouter(<MachinesPage />);

		expect(await screen.findByText("1 of 2 online")).toBeInTheDocument();
		expect(
			screen.getByRole("heading", { name: "1 machine waits for approval" }),
		).toBeInTheDocument();
		expect(screen.getAllByRole("row")).toHaveLength(3);
	});
});
