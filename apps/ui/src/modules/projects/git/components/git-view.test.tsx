import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { GitView } from "@/modules/projects/git/components/git-view";
import type {
	GitChange,
	GitDiffGroup,
	GitStatus,
	Project,
} from "@/shared/api/generated/ahab.schemas";
import {
	getGitBranchesMockHandler,
	getGitLogMockHandler,
	getGitStatusMockHandler,
} from "@/shared/api/generated/projects-git/projects-git.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const project = {
	id: "p",
	name: "app",
	path: "/work/app",
	machineId: "m1",
	machineName: "atlas",
	mode: "readWrite",
	availability: "available",
} as Project;

const change = (path: string, status = "modified"): GitChange => ({
	path,
	status,
	added: 2,
	removed: 1,
});

const status: GitStatus = {
	repository: true,
	prefix: "",
	branch: "main",
	head: "abc1234",
	upstream: "origin/main",
	ahead: 1,
	behind: 0,
	staged: [change("src/lib.rs")],
	changes: [change("src/main.rs")],
	untracked: [change("notes.md", "untracked")],
	conflicts: [],
};

const commit = (hash: string, subject: string, parents: string[], refs: string[] = []) => ({
	hash,
	short: hash.slice(0, 7),
	author: "Ada",
	email: "ada@example.com",
	time: Math.floor(Date.now() / 1000) - 3600,
	subject,
	parents,
	refs,
});

describe("GitView", () => {
	beforeEach(() => {
		mockServer.use(
			getGitLogMockHandler({
				commits: [
					commit(
						"m000000",
						"Merge feature",
						["b000000", "f000000"],
						["HEAD", "main", "origin/main"],
					),
					commit("f000000", "Feature work", ["a000000"], ["feature", "tag: v1"]),
					commit("b000000", "Fix", ["a000000"]),
					commit("a000000", "First", []),
				],
				more: false,
			}),
			getGitBranchesMockHandler({
				current: "main",
				local: [],
				remote: [{ name: "origin/main", commit: "m000000", subject: "", time: 0 }],
			}),
		);
	});

	it("lists changes once, checked when staged, opens diffs and stages files", async () => {
		const staged: unknown[] = [];
		const unstaged: unknown[] = [];
		mockServer.use(
			getGitStatusMockHandler(status),
			http.post("*/api/v1/projects/p/git/stage", async ({ request }) => {
				staged.push(await request.json());
				return HttpResponse.json({ output: "" });
			}),
			http.post("*/api/v1/projects/p/git/unstage", async ({ request }) => {
				unstaged.push(await request.json());
				return HttpResponse.json({ output: "" });
			}),
		);
		const onOpenDiff = vi.fn<(change: GitChange, group: GitDiffGroup, commit?: string) => void>();
		renderWithQuery(<GitView project={project} onOpenDiff={onOpenDiff} onOpenFile={() => {}} />);

		const changes = await screen.findByRole("region", { name: "Changes" });
		expect(within(changes).getByRole("checkbox", { name: "Unstage src/lib.rs" })).toBeChecked();
		expect(within(changes).getByRole("checkbox", { name: "Stage src/main.rs" })).not.toBeChecked();
		expect(within(changes).getByRole("checkbox", { name: "Stage all" })).toHaveAttribute(
			"aria-checked",
			"mixed",
		);

		await userEvent.click(within(changes).getByText("notes.md"));
		expect(onOpenDiff).toHaveBeenCalledWith(
			expect.objectContaining({ path: "notes.md" }),
			"untracked",
		);

		await userEvent.click(within(changes).getByRole("checkbox", { name: "Stage src/main.rs" }));
		await vi.waitFor(() => expect(staged).toEqual([{ paths: ["src/main.rs"] }]));
		await userEvent.click(within(changes).getByRole("checkbox", { name: "Unstage src/lib.rs" }));
		await vi.waitFor(() => expect(unstaged).toEqual([{ paths: ["src/lib.rs"] }]));

		// With changes, the main button commits; the branch is in the header.
		expect(screen.getByRole("button", { name: "Commit" })).toBeDisabled();
		expect(screen.getByRole("button", { name: /Branch main/ })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Sync: 0 to pull, 1 to push" })).toBeInTheDocument();
	});

	it("offers to sync, or to publish a branch, when there is nothing to commit", async () => {
		mockServer.use(getGitStatusMockHandler({ ...status, staged: [], changes: [], untracked: [] }));
		const { unmount } = renderWithQuery(
			<GitView project={project} onOpenDiff={() => {}} onOpenFile={() => {}} />,
		);
		expect(await screen.findByRole("button", { name: /Sync changes/ })).toBeEnabled();
		unmount();
		mockServer.use(
			getGitStatusMockHandler({
				...status,
				upstream: null,
				staged: [],
				changes: [],
				untracked: [],
			}),
		);
		renderWithQuery(<GitView project={project} onOpenDiff={() => {}} onOpenFile={() => {}} />);
		expect(await screen.findByRole("button", { name: "Publish branch" })).toBeEnabled();
	});

	it("shows changes as a tree of folders on request", async () => {
		mockServer.use(getGitStatusMockHandler(status));
		renderWithQuery(<GitView project={project} onOpenDiff={() => {}} onOpenFile={() => {}} />);
		const changes = await screen.findByRole("region", { name: "Changes" });
		await userEvent.click(within(changes).getByRole("button", { name: "View as a tree" }));
		const folder = within(changes).getByRole("checkbox", { name: "Stage src" });
		expect(folder).toHaveAttribute("aria-checked", "mixed");
		await userEvent.click(within(changes).getByRole("button", { name: "Fold src" }));
		expect(within(changes).queryByText("main.rs")).not.toBeInTheDocument();
	});

	it("makes a folder a repository only after asking", async () => {
		const inits: unknown[] = [];
		mockServer.use(
			getGitStatusMockHandler({ ...status, repository: false }),
			http.post("*/api/v1/projects/p/git/init", () => {
				inits.push(true);
				return HttpResponse.json({ output: "" });
			}),
		);
		renderWithQuery(<GitView project={project} onOpenDiff={() => {}} onOpenFile={() => {}} />);
		await userEvent.click(
			await screen.findByRole("button", { name: "Initialize a repository here…" }),
		);
		expect(inits).toEqual([]);
		const dialog = screen.getByRole("alertdialog");
		expect(within(dialog).getByText(/git init in \/work\/app on atlas/)).toBeInTheDocument();
		await userEvent.click(within(dialog).getByRole("button", { name: "Initialize" }));
		await vi.waitFor(() => expect(inits).toEqual([true]));
	});

	it("works in a repository inside the folder, and switches between several", async () => {
		const asked: string[] = [];
		mockServer.use(
			http.get("*/api/v1/projects/p/git/status", ({ request }) => {
				asked.push(new URL(request.url).searchParams.get("repository") ?? "");
				return HttpResponse.json(status);
			}),
		);
		const onChooseRepository = vi.fn<(path: string) => void>();
		renderWithQuery(
			<GitView
				project={project}
				repository="ahab"
				repositories={[
					{ path: "ahab", branch: "main" },
					{ path: "tools", branch: "feat/probe" },
				]}
				onChooseRepository={onChooseRepository}
				onOpenDiff={() => {}}
				onOpenFile={() => {}}
			/>,
		);
		await screen.findByRole("region", { name: "Changes" });
		expect(asked).toContain("ahab");
		expect(screen.getByRole("region", { name: "Repository ahab" })).toBeInTheDocument();
		const tools = screen.getByRole("region", { name: "Repository tools" });
		// Folded: its header shows its branch, not its changes.
		expect(within(tools).queryByRole("region", { name: "Changes" })).not.toBeInTheDocument();
		expect(within(tools).getByText("feat/probe")).toBeInTheDocument();
		expect(asked).not.toContain("tools");
		await userEvent.click(within(tools).getByRole("button", { name: "Unfold tools" }));
		expect(onChooseRepository).toHaveBeenCalledWith("tools");
		const opened = screen.getByRole("region", { name: "Repository tools" });
		expect(await within(opened).findByRole("region", { name: "Changes" })).toBeInTheDocument();
	});

	it("groups a repository's worktrees under it", async () => {
		mockServer.use(getGitStatusMockHandler(status));
		renderWithQuery(
			<GitView
				project={project}
				repository="clients/app/main"
				repositories={[
					{ path: "api", branch: "main", repository: "api" },
					{ path: "clients/app/fix", branch: "fix", repository: "clients/app" },
					{ path: "clients/app/main", branch: "main", repository: "clients/app" },
					{ path: "lib/x", branch: "main", repository: "lib" },
					{ path: "lib/y", branch: "dev", repository: "lib" },
				]}
				onOpenDiff={() => {}}
				onOpenFile={() => {}}
			/>,
		);
		const app = await screen.findByRole("region", { name: "Repository clients/app" });
		expect(within(app).getByText("2 worktrees")).toBeInTheDocument();
		expect(within(app).getByRole("region", { name: "Repository fix" })).toBeInTheDocument();
		expect(await within(app).findByRole("region", { name: "Repository main" })).toBeInTheDocument();
		await userEvent.click(within(app).getByRole("button", { name: "Fold clients/app" }));
		expect(within(app).queryByRole("region", { name: "Repository fix" })).not.toBeInTheDocument();
		// Others start folded, and unfold on request.
		const lib = screen.getByRole("region", { name: "Repository lib" });
		expect(within(lib).queryByRole("region", { name: "Repository x" })).not.toBeInTheDocument();
		await userEvent.click(within(lib).getByRole("button", { name: "Unfold lib" }));
		expect(within(lib).getByRole("region", { name: "Repository y" })).toBeInTheDocument();
	});

	it("draws the graph with branches and tags on their commits", async () => {
		mockServer.use(getGitStatusMockHandler(status));
		renderWithQuery(<GitView project={project} onOpenDiff={() => {}} onOpenFile={() => {}} />);
		const graph = await screen.findByRole("region", { name: "Graph" });
		const merge = (await within(graph).findByText("Merge feature")).closest(
			"button",
		) as HTMLElement;
		// main is current and in step with origin/main, which is left out.
		expect(within(merge).getByText("main")).toBeInTheDocument();
		expect(within(merge).queryByText("origin/main")).not.toBeInTheDocument();
		const feature = within(graph).getByText("Feature work").closest("button") as HTMLElement;
		expect(within(feature).getByText("feature")).toBeInTheDocument();
		expect(within(feature).getByText("+1")).toHaveAttribute("title", "v1");
		await userEvent.click(feature);
		expect(await within(graph).findByText("v1")).toBeInTheDocument();
	});

	it("marks the files the open session's agents edited, and filters to them", async () => {
		mockServer.use(getGitStatusMockHandler(status));
		renderWithQuery(
			<GitView
				project={project}
				edited={["/work/app/src/main.rs", "/elsewhere/x.rs"]}
				onOpenDiff={() => {}}
				onOpenFile={() => {}}
			/>,
		);
		const changes = await screen.findByRole("region", { name: "Changes" });
		expect(within(changes).getAllByLabelText("Edited in this session")).toHaveLength(1);
		await userEvent.click(within(changes).getByRole("button", { name: "Only this session's" }));
		expect(within(changes).getByText("main.rs")).toBeInTheDocument();
		expect(within(changes).queryByText("notes.md")).not.toBeInTheDocument();
	});
});
