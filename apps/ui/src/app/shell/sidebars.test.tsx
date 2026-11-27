import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ActivityBar, type ViewId } from "@/app/shell/sidebars";

describe("ActivityBar", () => {
	it("shows a view, and hides the sidebar when its shown view is clicked again", async () => {
		const onShow = vi.fn<(view: ViewId) => void>();
		const onHide = vi.fn<() => void>();
		render(
			<ActivityBar
				side="left"
				bar={{ views: ["sessions", "files"], active: "sessions", open: true, width: 280 }}
				onShow={onShow}
				onHide={onHide}
				onMove={() => {}}
			/>,
		);
		await userEvent.click(screen.getByRole("button", { name: "Files" }));
		expect(onShow).toHaveBeenCalledWith("files");
		await userEvent.click(screen.getByRole("button", { name: "Sessions", pressed: true }));
		expect(onHide).toHaveBeenCalledOnce();
	});

	it("shows the active view of a closed sidebar instead of hiding it", async () => {
		const onShow = vi.fn<(view: ViewId) => void>();
		const onHide = vi.fn<() => void>();
		render(
			<ActivityBar
				side="right"
				bar={{ views: ["git"], active: "git", open: false, width: 280 }}
				onShow={onShow}
				onHide={onHide}
				onMove={() => {}}
			/>,
		);
		await userEvent.click(screen.getByRole("button", { name: "Git" }));
		expect(onShow).toHaveBeenCalledWith("git");
		expect(onHide).not.toHaveBeenCalled();
	});
});
