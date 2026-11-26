import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { AddMachineMenu } from "@/modules/machines/enrollment/components/add-machine-menu";
import { renderWithQuery } from "@/testing/render";

describe("AddMachineMenu", () => {
	it("offers SSH first, a one-time code, and links the other way to the docs", async () => {
		const onSsh = vi.fn<() => void>();
		const onCode = vi.fn<() => void>();
		renderWithQuery(<AddMachineMenu onSsh={onSsh} onCode={onCode} />);

		await userEvent.click(screen.getByRole("button", { name: /Add machine/ }));
		expect(screen.getByRole("menuitem", { name: /With approval/ })).toHaveAttribute(
			"href",
			"https://useahab.com/docs/installation/cli/#connect-another-machine",
		);
		const items = screen.getAllByRole("menuitem");
		expect(items[0]).toHaveAccessibleName(/Over SSH \(recommended\)/);
		await userEvent.click(screen.getByRole("menuitem", { name: /With a one-time code/ }));
		expect(onCode).toHaveBeenCalledOnce();
		expect(onSsh).not.toHaveBeenCalled();
	});

	it("opens the SSH way", async () => {
		const onSsh = vi.fn<() => void>();
		renderWithQuery(<AddMachineMenu onSsh={onSsh} onCode={vi.fn<() => void>()} />);

		await userEvent.click(screen.getByRole("button", { name: /Add machine/ }));
		await userEvent.click(screen.getByRole("menuitem", { name: /Over SSH/ }));
		expect(onSsh).toHaveBeenCalledOnce();
	});
});
