import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { NotificationBell } from "@/modules/notifications/notification/components/notification-bell";
import type { NotificationList } from "@/shared/api/generated/ahab.schemas";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const inbox: NotificationList = {
	unread: 2,
	items: [
		{
			id: "n1",
			kind: "sessions.waiting",
			source: "ahab",
			level: "warning",
			title: "Claude Code waits for you",
			body: "Allow running cargo test?",
			target: {},
			createdAt: new Date().toISOString(),
		},
		{
			id: "n2",
			kind: "sessions.finished",
			source: "ahab",
			level: "success",
			title: "Codex finished",
			target: {},
			createdAt: new Date().toISOString(),
		},
	],
};

function serve() {
	mockServer.use(http.get("*/api/v1/notifications", () => HttpResponse.json(inbox)));
}

describe("NotificationBell", () => {
	it("shows the unread count and opens the inbox in a popover", async () => {
		serve();
		renderWithRouter(<NotificationBell />);
		const bell = await screen.findByRole("button", { name: "Notifications, 2 unread" });
		expect(bell).toHaveTextContent("2");
		await userEvent.click(bell);
		const list = await screen.findByRole("region", { name: "Notifications" });
		expect(within(list).getByText("Claude Code waits for you")).toBeVisible();
		expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
	});

	it("on a phone, opens the inbox as a full-height sheet with 44px controls that closes", async () => {
		serve();
		renderWithRouter(<NotificationBell phone />);
		const bell = await screen.findByRole("button", { name: "Notifications, 2 unread" });
		expect(bell).toHaveClass("size-11");
		await userEvent.click(bell);
		const sheet = await screen.findByRole("dialog", { name: "Notifications" });
		expect(sheet.className).toContain("100svh");
		expect(within(sheet).getByText("2 unread")).toBeVisible();
		expect(within(sheet).getByRole("button", { name: "Mark all read" })).toHaveClass("h-11");
		await userEvent.click(within(sheet).getByRole("button", { name: "Close" }));
		expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
	});
});
