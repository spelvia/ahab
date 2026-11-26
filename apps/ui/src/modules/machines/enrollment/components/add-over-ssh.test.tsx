import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { AddOverSshPanel } from "@/modules/machines/enrollment/components/add-over-ssh";
import { renderWithQuery } from "@/testing/render";

describe("AddOverSshPanel", () => {
	it("shows the command to copy, and closes", async () => {
		const onClose = vi.fn<() => void>();
		renderWithQuery(<AddOverSshPanel onClose={onClose} />);

		expect(screen.getByRole("heading", { name: "Add a machine over SSH" })).toBeInTheDocument();
		expect(screen.getByText("ahab machines add you@host")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Copy SSH command" })).toBeInTheDocument();
		expect(screen.getByRole("link", { name: /More in the docs/ })).toHaveAttribute(
			"href",
			"https://useahab.com/docs/installation/cli/#add-a-machine-over-ssh",
		);
		await userEvent.click(screen.getByRole("button", { name: "Close" }));
		expect(onClose).toHaveBeenCalledOnce();
	});
});
