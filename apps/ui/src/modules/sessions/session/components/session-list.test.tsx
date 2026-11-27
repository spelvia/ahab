import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { SessionsAddMenu } from "@/modules/sessions/session/components/add-menu";
import { NewSession } from "@/modules/sessions/session/components/new-session";
import { SessionList } from "@/modules/sessions/session/components/session-list";
import type { Session, SessionFolder, Project } from "@/shared/api/generated/ahab.schemas";
import {
	getListSessionFoldersMockHandler,
	getListSessionsMockHandler,
} from "@/shared/api/generated/sessions-session/sessions-session.msw";
import { Toaster } from "@/shared/ui/toast";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const session = (id: string, title: string, change: Partial<Session>): Session => ({
	id,
	kind: "chat",
	terminal: null,
	terminals: [],
	title,
	projectId: "p",
	state: "working",
	waiting: 0,
	chats: [],
	createdAt: "2026-11-14T09:00:00Z",
	updatedAt: new Date().toISOString(),
	folderId: null,
	pinnedAt: null,
	archivedAt: null,
	...change,
});

const terminalSession = (id: string, title: string): Session => {
	const shown = session(id, title, {
		kind: "terminal",
		state: "idle",
		terminal: {
			id: `${id}-t`,
			sessionId: id,
			state: "running",
			exitCode: null,
			hookedBy: [],
			machineId: "m",
			projectId: "w1",
			cols: 80,
			rows: 24,
			title: null,
			createdAt: "2026-11-14T09:00:00Z",
			updatedAt: "2026-11-14T09:00:00Z",
		},
	});
	return { ...shown, terminals: shown.terminal ? [shown.terminal] : [] };
};

const project = {
	id: "p",
	name: "app",
	path: "/work/app",
	machineId: "m1",
	machineName: "atlas",
	mode: "readWrite",
	availability: "available",
} as Project;

const folder = {
	id: "f1",
	projectId: "p",
	name: "Auth rewrite",
	createdAt: "2026-11-14T09:00:00Z",
	updatedAt: "2026-11-14T09:00:00Z",
};

describe("SessionList", () => {
	it("previews on click and keeps a session on double-click", async () => {
		const item = session("a", "Current work", { state: "idle" });
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [] }),
			getListSessionsMockHandler({ items: [item] }),
		);
		const onOpen = vi.fn<(session: Session, keep?: boolean) => void>();
		renderWithQuery(<SessionList project="p" onOpen={onOpen} />);
		const row = await screen.findByText("Current work");
		await userEvent.click(row);
		expect(onOpen).toHaveBeenLastCalledWith(item);
		await userEvent.dblClick(row);
		expect(onOpen).toHaveBeenLastCalledWith(item, true);
	});

	it("selects several with ⌘ and Shift, and pins them all at once", async () => {
		const items = [
			session("a", "First", { state: "idle", updatedAt: "2026-11-14T12:00:00Z" }),
			session("b", "Second", { state: "idle", updatedAt: "2026-11-14T11:00:00Z" }),
			session("c", "Third", { state: "idle", updatedAt: "2026-11-14T10:00:00Z" }),
		];
		const changed: unknown[] = [];
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [] }),
			getListSessionsMockHandler({ items }),
			http.patch("*/api/v1/sessions/:id", async ({ params, request }) => {
				changed.push([params.id, await request.json()]);
				return HttpResponse.json(items.find((item) => item.id === params.id));
			}),
		);
		const onOpen = vi.fn<(session: Session, keep?: boolean) => void>();
		renderWithQuery(<SessionList project="p" onOpen={onOpen} />);
		await userEvent.click(await screen.findByText("First"));
		expect(onOpen).toHaveBeenCalledTimes(1);

		const user = userEvent.setup();
		await user.keyboard("{Shift>}");
		await user.click(screen.getByText("Third"));
		await user.keyboard("{/Shift}");
		// Selecting does not open.
		expect(onOpen).toHaveBeenCalledTimes(1);
		const bar = screen.getByRole("toolbar", { name: "3 selected sessions" });
		await user.keyboard("{Control>}");
		await user.click(screen.getByText("Second"));
		await user.keyboard("{/Control}");
		expect(within(bar).getByText("2 selected")).toBeInTheDocument();

		await user.click(within(bar).getByRole("button", { name: "Pin" }));
		await vi.waitFor(() => expect(changed).toHaveLength(2));
		expect(changed).toEqual(
			expect.arrayContaining([
				["a", { pinned: true }],
				["c", { pinned: true }],
			]),
		);
		expect(screen.queryByRole("toolbar")).not.toBeInTheDocument();
	});

	it("groups by status when asked, and remembers it", async () => {
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [folder] }),
			getListSessionsMockHandler({
				items: [
					session("a", "Asking", { state: "needsYou" }),
					session("b", "Filed", { state: "idle", folderId: "f1" }),
				],
			}),
		);
		renderWithQuery(<SessionList project="p" onOpen={() => {}} />);
		await screen.findByText("Asking");
		await userEvent.click(screen.getByRole("button", { name: "Sort and group sessions" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Status" }));
		const asking = screen.getByRole("region", { name: "needs you" });
		expect(within(asking).getByText("Asking")).toBeInTheDocument();
		// Grouped by status, folders give way.
		expect(screen.queryByRole("region", { name: "Folder Auth rewrite" })).not.toBeInTheDocument();
		expect(
			within(screen.getByRole("region", { name: "idle" })).getByText("Filed"),
		).toBeInTheDocument();

		// Back to folders (kept on this device, like the choice above).
		await userEvent.click(screen.getByRole("button", { name: "Sort and group sessions" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Folders" }));
		expect(screen.getByRole("region", { name: "Folder Auth rewrite" })).toBeInTheDocument();
	});

	it("lists pinned sessions first, then folders, then the rest; archived ones apart", async () => {
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [folder] }),
			getListSessionsMockHandler({
				items: [
					session("a", "Loose work", { state: "idle" }),
					session("b", "Filed work", { folderId: "f1" }),
					session("c", "Pinned work", { pinnedAt: "2026-11-14T10:00:00Z" }),
					session("d", "Old work", { state: "ended", archivedAt: "2026-11-14T11:00:00Z" }),
				],
			}),
		);
		renderWithQuery(<SessionList project="p" onOpen={() => {}} />);

		const pinned = await screen.findByRole("list", { name: "Pinned sessions" });
		expect(within(pinned).getByText("Pinned work")).toBeInTheDocument();
		const filed = screen.getByRole("region", { name: "Folder Auth rewrite" });
		expect(within(filed).getByText("Filed work")).toBeInTheDocument();
		expect(
			within(screen.getByRole("list", { name: "Sessions" })).getByText("Loose work"),
		).toBeInTheDocument();
		expect(screen.queryByText("Old work")).not.toBeInTheDocument();

		// Folders close; archived sessions open on request.
		await userEvent.click(
			within(filed).getByRole("button", { name: "Auth rewrite", expanded: true }),
		);
		expect(screen.queryByText("Filed work")).not.toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Archived (1)" }));
		expect(screen.getByText("Old work")).toBeInTheDocument();
	});

	it("archives an idle session at once, and offers to undo it", async () => {
		const calls: string[] = [];
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [] }),
			getListSessionsMockHandler({ items: [session("a", "Loose work", { state: "idle" })] }),
			http.post("*/api/v1/sessions/:id/archive", ({ params }) => {
				calls.push(`archive ${String(params.id)}`);
				return HttpResponse.json(session("a", "Loose work", { state: "ended" }));
			}),
			http.post("*/api/v1/sessions/:id/unarchive", ({ params }) => {
				calls.push(`unarchive ${String(params.id)}`);
				return HttpResponse.json(session("a", "Loose work", { state: "ended" }));
			}),
		);
		renderWithQuery(
			<>
				<SessionList project="p" onOpen={() => {}} />
				<Toaster />
			</>,
		);
		await userEvent.click(await screen.findByRole("button", { name: "Archive Loose work" }));
		expect(await screen.findByText("Session archived")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Undo" }));
		await vi.waitFor(() => expect(calls).toEqual(["archive a", "unarchive a"]));
	});

	it("shows the project's sessions with what they need, and opens one", async () => {
		mockServer.use(
			getListSessionsMockHandler({
				items: [
					session("a", "Fix the flaky test", { state: "needsYou", waiting: 1 }),
					session("b", "Write the docs", {}),
					session("c", "Old work", { state: "ended" }),
				],
			}),
		);
		const onOpen = vi.fn<(session: Session) => void>();
		renderWithQuery(<SessionList project="p" onOpen={onOpen} />);

		expect(await screen.findByText("Fix the flaky test")).toBeInTheDocument();
		expect(screen.getByText("needs you")).toBeInTheDocument();
		await userEvent.click(screen.getByText("Write the docs"));
		expect(onOpen.mock.calls[0]?.[0].id).toBe("b");
	});

	it("says how to start when there are none", async () => {
		mockServer.use(getListSessionsMockHandler({ items: [] }));
		renderWithQuery(<SessionList project="p" onOpen={() => {}} />);
		expect(await screen.findByText(/No sessions yet/)).toBeInTheDocument();
	});

	it("lists agents and terminals together, each saying which it is, with empty folders", async () => {
		localStorage.clear(); // an earlier test closed the folder
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [folder] }),
			getListSessionsMockHandler({
				items: [session("a", "Agent work", {}), terminalSession("b", "A shell")],
			}),
		);
		renderWithQuery(<SessionList project="p" onOpen={() => {}} />);
		const list = await screen.findByRole("list", { name: "Sessions" });
		expect(within(list).getByText("Agent work")).toBeInTheDocument();
		expect(within(list).getByText("A shell")).toBeInTheDocument();
		expect(within(list).getByText("Terminal")).toBeInTheDocument();
		const empty = screen.getByRole("region", { name: "Folder Auth rewrite" });
		expect(within(empty).getByText(/Drag sessions here/)).toBeInTheDocument();
	});
});

describe("SessionsAddMenu", () => {
	it("starts an agent or a terminal, or makes a folder", async () => {
		const folders: string[] = [];
		mockServer.use(
			http.post("*/api/v1/projects/:project/session-folders", async ({ request }) => {
				const body = (await request.json()) as { name: string };
				folders.push(body.name);
				return HttpResponse.json({ ...folder, name: body.name }, { status: 201 });
			}),
		);
		const onNewAgent = vi.fn<() => void>();
		const onNewTerminal = vi.fn<() => void>();
		renderWithQuery(
			<SessionsAddMenu project="p" onNewAgent={onNewAgent} onNewTerminal={onNewTerminal} />,
		);
		await userEvent.click(screen.getByRole("button", { name: "New…" }));
		await userEvent.click(screen.getByRole("menuitem", { name: "New terminal" }));
		expect(onNewTerminal).toHaveBeenCalledOnce();
		await userEvent.click(screen.getByRole("button", { name: "New…" }));
		await userEvent.click(screen.getByRole("menuitem", { name: "New agent session" }));
		expect(onNewAgent).toHaveBeenCalledOnce();
		await userEvent.click(screen.getByRole("button", { name: "New…" }));
		await userEvent.click(screen.getByRole("menuitem", { name: "New folder…" }));
		await userEvent.type(screen.getByLabelText("Name"), "Release{Enter}");
		await vi.waitFor(() => expect(folders).toEqual(["Release"]));
	});
});

describe("NewSession", () => {
	it("starts a terminal running a command when Terminal is chosen", async () => {
		const bodies: unknown[] = [];
		mockServer.use(
			http.post("*/api/v1/projects/:id/terminals", async ({ request }) => {
				bodies.push(await request.json());
				return HttpResponse.json(terminalSession("t", "pnpm dev"), { status: 201 });
			}),
		);
		const onStarted = vi.fn<(session: Session) => void>();
		renderWithQuery(<NewSession project={project} onStarted={onStarted} />);
		await userEvent.click(screen.getByRole("radio", { name: "Terminal" }));
		await userEvent.type(screen.getByLabelText(/Command/), "pnpm dev");
		await userEvent.click(screen.getByRole("button", { name: "Start terminal" }));
		await vi.waitFor(() => expect(onStarted).toHaveBeenCalledOnce());
		expect(bodies).toEqual([{ cols: 100, rows: 30, command: "pnpm dev", folderId: null }]);
	});

	it("files what it starts in its folder", async () => {
		const bodies: unknown[] = [];
		mockServer.use(
			http.post("*/api/v1/projects/:id/terminals", async ({ request }) => {
				bodies.push(await request.json());
				return HttpResponse.json(terminalSession("t", "Terminal"), { status: 201 });
			}),
		);
		renderWithQuery(
			<NewSession
				project={project}
				folder={{ id: "f1", name: "Auth rewrite" }}
				initialKind="terminal"
				onStarted={() => {}}
			/>,
		);
		expect(screen.getByText("In Auth rewrite")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Start terminal" }));
		await vi.waitFor(() =>
			expect(bodies).toEqual([{ cols: 100, rows: 30, command: null, folderId: "f1" }]),
		);
	});
});

describe("a folder's menu", () => {
	it("starts an agent or a terminal in the folder", async () => {
		localStorage.clear();
		mockServer.use(
			getListSessionFoldersMockHandler({ items: [folder] }),
			getListSessionsMockHandler({ items: [session("a", "Filed work", { folderId: "f1" })] }),
		);
		const onNewIn = vi.fn<(kind: "agent" | "terminal", folder?: SessionFolder) => void>();
		renderWithQuery(<SessionList project="p" onOpen={() => {}} onNewIn={onNewIn} />);
		await userEvent.click(
			await screen.findByRole("button", { name: "Folder Auth rewrite actions" }),
		);
		await userEvent.click(screen.getByRole("menuitem", { name: "New terminal here" }));
		expect(onNewIn).toHaveBeenCalledWith("terminal", folder);
		await userEvent.click(screen.getByRole("button", { name: "Folder Auth rewrite actions" }));
		await userEvent.click(screen.getByRole("menuitem", { name: "New agent session here" }));
		expect(onNewIn).toHaveBeenLastCalledWith("agent", folder);
	});
});
