import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { AddMachinePanel } from "@/modules/machines/enrollment/components/add-machine";
import {
	getCreateJoinCodeMockHandler,
	getListJoinCodesMockHandler,
} from "@/shared/api/generated/machines-enrollment/machines-enrollment.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const inAMinute = (minutes: number) => new Date(Date.now() + minutes * 60_000).toISOString();
const code = () => ({
	id: "c1",
	name: null,
	expiresAt: inAMinute(15),
	createdAt: new Date().toISOString(),
});
const command = "ahab setup --server https://ahab.example.internal --code ABCD-EFGH";

function renderPanel() {
	renderWithQuery(
		<AddMachinePanel server="https://ahab.example.internal" onClose={vi.fn<() => void>()} />,
	);
}

describe("AddMachinePanel", () => {
	beforeEach(() => sessionStorage.clear());

	it("creates a one-time code, shows the command and waits for a machine", async () => {
		mockServer.use(
			getCreateJoinCodeMockHandler({ ...code(), code: "ABCD-EFGH" }),
			getListJoinCodesMockHandler({ items: [code()] }),
		);
		renderPanel();

		expect(await screen.findByText(command)).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Copy command" })).toBeInTheDocument();
		expect(screen.getByText(/Waiting for a machine to use this code/)).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Withdraw code" })).toBeInTheDocument();
	});

	it("shows the same code after a refresh instead of creating another", async () => {
		sessionStorage.setItem(
			"ahab.join-code",
			JSON.stringify({ id: "c1", code: "ABCD-EFGH", expiresAt: inAMinute(10) }),
		);
		const create = vi.fn<() => void>();
		mockServer.use(
			getCreateJoinCodeMockHandler(() => {
				create();
				return { ...code(), id: "c2", code: "WXYZ-WXYZ" };
			}),
			getListJoinCodesMockHandler({ items: [code()] }),
		);
		renderPanel();

		expect(await screen.findByText(command)).toBeInTheDocument();
		expect(create).not.toHaveBeenCalled();
	});

	it("says a machine joined once the code is used", async () => {
		sessionStorage.setItem(
			"ahab.join-code",
			JSON.stringify({ id: "c1", code: "ABCD-EFGH", expiresAt: inAMinute(10) }),
		);
		mockServer.use(getListJoinCodesMockHandler({ items: [] }));
		renderPanel();

		expect(await screen.findByText(/A machine joined with this code/)).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Copy command" })).not.toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Add another" })).toBeInTheDocument();
	});
});
