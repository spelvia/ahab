import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { SessionList } from "@/modules/sessions/session/components/session-list";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { getSearchSessionsMockHandler } from "@/shared/api/generated/sessions-chat/sessions-chat.msw";
import {
	getListSessionFoldersMockHandler,
	getListSessionsMockHandler,
} from "@/shared/api/generated/sessions-session/sessions-session.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const session = (id: string, title: string): Session => ({
	id,
	kind: "chat",
	terminal: null,
	terminals: [],
	title,
	projectId: "p",
	state: "idle",
	waiting: 0,
	chats: [],
	createdAt: "2026-11-14T09:00:00Z",
	updatedAt: new Date().toISOString(),
	folderId: null,
	pinnedAt: null,
	archivedAt: null,
});

describe("searching sessions", () => {
	it("shows what the words find, marks them, and opens the session at the message", async () => {
		const asked: string[] = [];
		mockServer.use(
			getListSessionsMockHandler({ items: [session("s1", "Fix CI"), session("s2", "Docs")] }),
			getListSessionFoldersMockHandler({ items: [] }),
			getSearchSessionsMockHandler(({ request }) => {
				asked.push(new URL(request.url).searchParams.get("q") ?? "");
				return {
					items: [
						{
							sessionId: "s1",
							projectId: "w1",
							sessionTitle: "Fix CI",
							match: "agentMessage",
							chatId: "c1",
							seq: 7,
							snippet: "…the Postgres container starts late…",
							at: new Date().toISOString(),
						},
					],
				};
			}),
		);
		const onOpen = vi.fn<(session: Session, keep?: boolean) => void>();
		renderWithQuery(<SessionList project="p" onOpen={onOpen} />);

		await userEvent.click(await screen.findByRole("searchbox", { name: "Search sessions" }));
		await userEvent.paste("postgres");
		const result = await screen.findByRole("button", { name: /Fix CI.*Postgres container/ });
		expect(screen.getByText("Postgres").tagName).toBe("MARK");
		// The list of sessions is replaced by the results while searching.
		expect(screen.queryByText("Docs")).not.toBeInTheDocument();
		// What is typed settles before it is asked.
		expect(asked).toEqual(["postgres"]);

		await userEvent.click(result);
		expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "s1" }), true);

		await userEvent.click(screen.getByRole("button", { name: "Clear the search" }));
		expect(await screen.findByText("Docs")).toBeInTheDocument();
	});

	it("says when nothing is found", async () => {
		mockServer.use(
			getListSessionsMockHandler({ items: [session("s1", "Fix CI")] }),
			getListSessionFoldersMockHandler({ items: [] }),
			getSearchSessionsMockHandler({ items: [] }),
		);
		renderWithQuery(<SessionList project="p" onOpen={() => {}} />);
		await userEvent.type(await screen.findByRole("searchbox"), "kubernetes");
		expect(await screen.findByText("Nothing found for “kubernetes”.")).toBeInTheDocument();
	});
});
