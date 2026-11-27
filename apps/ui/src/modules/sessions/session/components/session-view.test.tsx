import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SessionPartPicker } from "@/modules/sessions/session/components/part-tabs";
import { SessionList } from "@/modules/sessions/session/components/session-list";
import { SessionView } from "@/modules/sessions/session/components/session-view";
import type { Chat, Session, Terminal } from "@/shared/api/generated/ahab.schemas";
import { getAgentCatalogMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import {
	getGetChatMockHandler,
	getUpdateChatMockHandler,
} from "@/shared/api/generated/sessions-chat/sessions-chat.msw";
import {
	getGetSessionMockHandler,
	getListSessionFoldersMockHandler,
	getListSessionsMockHandler,
} from "@/shared/api/generated/sessions-session/sessions-session.msw";
import { getStartSessionTerminalMockHandler } from "@/shared/api/generated/sessions-terminal/sessions-terminal.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

// Each tab's content is the chat's or terminal's own view, tested on its own.
vi.mock("@/modules/sessions/chat/components/chat-view", () => ({
	ChatView: ({ chatId }: { chatId: string }) => <p>conversation {chatId}</p>,
}));
vi.mock("@/modules/sessions/terminal/components/terminal-view", () => ({
	TerminalView: ({ terminal }: { terminal: Terminal }) => <p>terminal {terminal.id}</p>,
}));

const chat = (id: string, title: string, createdAt: string, change: Partial<Chat> = {}): Chat => ({
	id,
	sessionId: "s1",
	projectId: "p",
	machineId: "m",
	agent: "claude-code",
	title,
	state: "live",
	status: "idle",
	openRequests: [],
	resumable: true,
	unrestricted: false,
	createdAt,
	updatedAt: createdAt,
	...change,
});

const terminal: Terminal = {
	id: "t1",
	sessionId: "s1",
	projectId: "w1",
	machineId: "m",
	command: "pnpm test --watch",
	state: "running",
	exitCode: null,
	cols: 80,
	rows: 24,
	title: null,
	label: "tests",
	hookedBy: [],
	createdAt: "2026-11-19T09:02:00Z",
	updatedAt: "2026-11-19T09:02:00Z",
};

const session: Session = {
	id: "s1",
	kind: "chat",
	title: "Fix the flaky test",
	projectId: "p",
	state: "working",
	waiting: 0,
	// Most recent first, as the server gives them.
	chats: [
		chat("c2", "Second opinion", "2026-11-19T09:01:00Z", { agent: "codex" }),
		chat("c1", "Fix the flaky test", "2026-11-19T09:00:00Z", { status: "working" }),
	],
	terminal,
	terminals: [terminal],
	folderId: null,
	pinnedAt: null,
	archivedAt: null,
	createdAt: "2026-11-19T09:00:00Z",
	updatedAt: new Date().toISOString(),
};

beforeEach(() => {
	localStorage.clear();
	mockServer.use(
		getAgentCatalogMockHandler(),
		getGetChatMockHandler(chat("c1", "Fix the flaky test", "2026-11-19T09:00:00Z")),
	);
});

/** The session under another ID: each test its own window's view. */
const as = (id: string): Session => ({ ...session, id });

describe("a session's window", () => {
	it("toggles the session picker on click and activates the chosen part", async () => {
		const activate = vi.fn<() => void>();
		mockServer.use(getGetSessionMockHandler(as("s1")));
		renderWithQuery(
			<>
				<SessionPartPicker session={session} onActivate={activate}>
					<button>Session</button>
				</SessionPartPicker>
				<SessionView sessionId="s1" />
			</>,
		);
		const trigger = screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ });
		expect(await screen.findByText("terminal t1")).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Close tests" })).not.toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Add to this session" })).not.toBeInTheDocument();
		await userEvent.hover(trigger);
		expect(trigger).toHaveAttribute("title", "Show tabs in Fix the flaky test (3)");
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
		await userEvent.click(trigger);
		await userEvent.unhover(trigger);
		expect(trigger).toHaveAttribute("aria-expanded", "true");
		expect(trigger).toHaveAccessibleName("Hide tabs in Fix the flaky test (3)");
		await userEvent.click(trigger);
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
		await userEvent.click(trigger);
		const picker = await screen.findByRole("dialog");
		expect(activate).not.toHaveBeenCalled();
		expect(screen.getByText("terminal t1")).toBeInTheDocument();
		const entries = within(picker)
			.getAllByRole("button")
			.filter((button) => button.hasAttribute("title") && !button.getAttribute("aria-label"));
		expect(entries.map((item) => item.textContent)).toEqual([
			expect.stringContaining("Fix the flaky test"),
			expect.stringContaining("Second opinion"),
			expect.stringContaining("tests"),
		]);
		expect(entries[2]).toHaveAttribute("aria-current", "true");
		await userEvent.click(entries[0]!);
		expect(activate).toHaveBeenCalledOnce();
		expect(screen.getByText("conversation c1")).toBeInTheDocument();
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
	});

	it("opens the phone picker from the keyboard and returns focus on Escape", async () => {
		mockServer.use(getGetSessionMockHandler(as("phone")));
		renderWithQuery(<SessionPartPicker session={as("phone")} phone />);
		const trigger = await screen.findByRole("button", { name: /^Show tabs in Fix the flaky test/ });
		trigger.focus();
		await userEvent.keyboard("{Enter}");
		expect(await screen.findByRole("dialog")).toContainElement(
			document.activeElement as HTMLElement,
		);
		await userEvent.keyboard("{Escape}");
		expect(trigger).toHaveFocus();
	});

	it("closes a chat through its actions menu after confirmation", async () => {
		renderWithQuery(<SessionPartPicker session={as("close-chat")} />);
		await userEvent.click(screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ }));
		expect(screen.queryByRole("button", { name: /^Close/ })).not.toBeInTheDocument();
		const actions = screen.getByRole("button", { name: "Actions for Second opinion" });
		await userEvent.click(actions);
		await userEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
		const confirmation = await screen.findByRole("alertdialog", {
			name: "Close “Second opinion”?",
		});
		await userEvent.click(within(confirmation).getByRole("button", { name: "Cancel" }));
		expect(actions).toBeInTheDocument();
		await userEvent.click(actions);
		await userEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
		await userEvent.click(
			within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Close tab" }),
		);
		expect(actions).not.toBeInTheDocument();
	});

	it("saves a chat title from its visible actions menu", async () => {
		const renamed = vi.fn<(id: string, data: unknown) => void>();
		mockServer.use(
			getUpdateChatMockHandler(async ({ params, request }) => {
				renamed(String(params.id), await request.json());
				return { ...session.chats[0]!, title: "Review" };
			}),
		);
		renderWithQuery(<SessionPartPicker session={as("rename-chat")} />);
		await userEvent.click(screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ }));
		await userEvent.click(screen.getByRole("button", { name: "Actions for Second opinion" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Rename…" }));
		const field = await screen.findByRole("textbox", { name: "Name" });
		await userEvent.clear(field);
		await userEvent.type(field, "Review");
		await userEvent.click(screen.getByRole("button", { name: "Rename" }));
		await vi.waitFor(() => expect(renamed).toHaveBeenCalledWith("c2", { title: "Review" }));
	});

	it("asks before removing a running terminal and leaves it open on cancel", async () => {
		mockServer.use(getGetSessionMockHandler({ ...as("s2"), chats: [], terminals: [terminal] }));
		renderWithQuery(
			<>
				<SessionPartPicker session={as("s2")} />
				<SessionView sessionId="s2" />
			</>,
		);
		await userEvent.click(screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ }));
		await userEvent.click(await screen.findByRole("button", { name: "Actions for tests" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
		expect(screen.getByText(/This stops the terminal/)).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
		expect(screen.getByText("terminal t1")).toBeInTheDocument();
	});

	it("shows a second tab beside the first", async () => {
		mockServer.use(getGetSessionMockHandler(as("s3")));
		renderWithQuery(
			<>
				<SessionPartPicker session={as("s3")} />
				<SessionView sessionId="s3" />
			</>,
		);
		const trigger = screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ });
		await userEvent.click(trigger);
		const first = within(await screen.findByRole("dialog")).getByRole("button", {
			name: /^Fix the flaky test/,
		});
		fireEvent.contextMenu(first);
		await userEvent.click(await screen.findByRole("menuitem", { name: "Show beside" }));
		expect(screen.getByText("terminal t1")).toBeInTheDocument();
		expect(screen.getByText("conversation c1")).toBeInTheDocument();

		await userEvent.click(trigger);
		fireEvent.contextMenu(
			within(await screen.findByRole("dialog")).getByRole("button", {
				name: /^Fix the flaky test/,
			}),
		);
		await userEvent.click(await screen.findByRole("menuitem", { name: "Close the split" }));
		expect(screen.queryByText("conversation c1")).not.toBeInTheDocument();
	});

	it("adds a chat or a terminal to the session from +", async () => {
		const asked: string[] = [];
		mockServer.use(
			getGetSessionMockHandler(as("s4")),
			getStartSessionTerminalMockHandler(({ params }) => {
				asked.push(String(params.id));
				return { ...terminal, id: "t2", label: null };
			}),
		);
		renderWithQuery(
			<>
				<SessionPartPicker session={as("s4")} />
				<SessionView sessionId="s4" />
			</>,
		);
		await userEvent.click(screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ }));
		await userEvent.click(await screen.findByRole("button", { name: "Add to this session" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "New terminal" }));
		await vi.waitFor(() => expect(asked).toEqual(["s4"]));

		await userEvent.click(screen.getByRole("button", { name: /^Show tabs in Fix the flaky test/ }));
		await userEvent.click(screen.getByRole("button", { name: "Add to this session" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "New chat" }));
		expect(
			await screen.findByText("In “Fix the flaky test”, beside what it holds."),
		).toBeInTheDocument();
	});
});

describe("the Sessions view's tree", () => {
	it("opens a session to list its chats and terminals, and opens one at its tab", async () => {
		mockServer.use(
			getListSessionsMockHandler({ items: [session] }),
			getListSessionFoldersMockHandler({ items: [] }),
		);
		const onOpen = vi.fn<(session: Session, keep?: boolean) => void>();
		renderWithQuery(<SessionList project="p" onOpen={onOpen} />);

		expect(await screen.findByText("+2 more")).toBeInTheDocument();
		await userEvent.click(
			screen.getByRole("button", { name: "Show what Fix the flaky test holds" }),
		);
		const held = screen.getByRole("list", { name: "In this session" });
		expect(within(held).getAllByRole("listitem")).toHaveLength(3);
		expect(within(held).getByText("working")).toBeInTheDocument();

		await userEvent.click(within(held).getByText("Second opinion"));
		expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "s1" }), true);
		const kept = JSON.parse(localStorage.getItem("ahab.preferences.v1") ?? "{}");
		expect(kept.sessionViews.s1.active).toBe("chat:c2");
	});
});
