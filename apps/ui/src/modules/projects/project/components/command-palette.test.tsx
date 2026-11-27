import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { CommandPalette } from "@/modules/projects/project/components/command-palette";

describe("CommandPalette", () => {
	it("filters as you type and runs the chosen command", async () => {
		const homelab = vi.fn<() => void>();
		const open = vi.fn<() => void>();
		render(
			<CommandPalette
				open
				onOpenChange={() => {}}
				commands={[
					{ id: "p", group: "Project", label: "homelab", run: homelab },
					{ id: "o", group: "Action", label: "Open folder…", run: open },
				]}
			/>,
		);
		await userEvent.type(screen.getByRole("textbox", { name: "Search commands" }), "open");
		expect(screen.queryByText("homelab")).not.toBeInTheDocument();
		await userEvent.keyboard("{Enter}");
		expect(open).toHaveBeenCalled();
		expect(homelab).not.toHaveBeenCalled();
	});

	it("says when nothing matches", async () => {
		render(<CommandPalette open onOpenChange={() => {}} commands={[]} />);
		expect(screen.getByText("Nothing matches.")).toBeInTheDocument();
	});
});
