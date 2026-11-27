import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ProblemsPanel } from "@/modules/tools/problems/components/problems-panel";
import type { ProblemDto } from "@/shared/api/generated/ahab.schemas";
import { getGitRepositoriesMockHandler } from "@/shared/api/generated/projects-git/projects-git.msw";
import {
	getCheckRootsMockHandler,
	getListProblemsMockHandler,
	getListRepositoryActivityMockHandler,
	getPinRepositoryMockHandler,
	getRunChecksMockHandler,
} from "@/shared/api/generated/tools-problems/tools-problems.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

function problem(change: Partial<ProblemDto>): ProblemDto {
	return {
		path: "engine/src/main.rs",
		source: "rust-analyzer",
		severity: "error",
		line: 3,
		column: 1,
		endLine: 3,
		endColumn: 4,
		code: null,
		message: "mismatched types",
		...change,
	};
}

const project = { id: "w1", name: "spelvia", availability: "available" as const };

/** A check's root in a checkout of its own. */
function root(checkout: string) {
	return { tool: "gradle", name: "Gradle", root: checkout, checkout };
}

describe("ProblemsPanel", () => {
	it("groups by repository and file, filters, opens and runs the checks", async () => {
		mockServer.use(
			getGitRepositoriesMockHandler({
				inside: false,
				repositories: [{ path: "engine" }, { path: "ui" }],
			}),
			getListProblemsMockHandler({
				counts: { errors: 1, warnings: 1, information: 0, hints: 0 },
				items: [
					problem({}),
					problem({
						path: "ui/src/app.ts",
						severity: "warning",
						source: "eslint",
						line: 7,
						message: "unused variable",
					}),
				],
				updatedAt: null,
			}),
			getRunChecksMockHandler({
				checks: [
					{
						tool: "cargo-clippy",
						name: "cargo clippy",
						root: "engine",
						ok: false,
						message: "cargo is not installed",
						counts: { errors: 0, warnings: 0, information: 0, hints: 0 },
					},
				],
				counts: { errors: 1, warnings: 1, information: 0, hints: 0 },
			}),
		);
		const onOpenFile = vi.fn<(projectId: string, path: string, line?: number) => void>();
		renderWithQuery(<ProblemsPanel project={project} toolbar={null} onOpenFile={onOpenFile} />);
		expect(await screen.findByText("main.rs")).toBeInTheDocument();
		expect(await screen.findByRole("button", { name: /^engine/ })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /^ui/ })).toBeInTheDocument();

		await userEvent.type(screen.getByRole("textbox", { name: "Filter problems" }), "**/*.ts");
		expect(screen.queryByText("mismatched types")).not.toBeInTheDocument();
		expect(screen.getByText("unused variable")).toBeInTheDocument();
		await userEvent.clear(screen.getByRole("textbox", { name: "Filter problems" }));
		await userEvent.type(
			screen.getByRole("textbox", { name: "Filter problems" }),
			"nothing like it",
		);
		expect(screen.getByText(/No results found with the filter/)).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Clear the filter" }));

		await userEvent.click(screen.getByText("mismatched types"));
		expect(onOpenFile).toHaveBeenCalledWith("w1", "engine/src/main.rs", 3);

		await userEvent.click(screen.getByRole("button", { name: "Collapse all" }));
		expect(screen.queryByText("main.rs")).not.toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
		expect(screen.getByText("main.rs")).toBeInTheDocument();

		await userEvent.click(screen.getByRole("button", { name: "Check project" }));
		expect(await screen.findByText(/cargo is not installed/)).toBeInTheDocument();
	});

	it("says when nothing was found, as VS Code does", async () => {
		mockServer.use(
			getGitRepositoriesMockHandler({ inside: true, repositories: [] }),
			getListProblemsMockHandler({
				counts: { errors: 0, warnings: 0, information: 0, hints: 0 },
				items: [],
				updatedAt: null,
			}),
		);
		renderWithQuery(<ProblemsPanel project={project} toolbar={null} onOpenFile={() => {}} />);
		expect(
			await screen.findByText("No problems have been detected in the project."),
		).toBeInTheDocument();
	});

	it("groups repository → worktree → file, shows the active ones and asks which to check", async () => {
		const ran: unknown[] = [];
		const pinned: unknown[] = [];
		mockServer.use(
			getGitRepositoriesMockHandler({
				inside: false,
				repositories: [
					{ path: "app/main", repository: "app", kind: "worktree", branch: "main" },
					{ path: "app/fix", repository: "app", kind: "worktree", branch: "fix" },
					...["a", "b", "c", "d", "e"].map((name) => ({ path: name, repository: name })),
				],
			}),
			getListRepositoryActivityMockHandler({
				activeDays: 14,
				items: [
					{
						repository: "app",
						worktree: "app/fix",
						pinned: false,
						active: true,
						lastActiveAt: null,
					},
					{ repository: "b", worktree: null, pinned: true, active: true, lastActiveAt: null },
				],
			}),
			getListProblemsMockHandler({
				counts: { errors: 2, warnings: 0, information: 0, hints: 0 },
				items: [
					problem({ path: "app/fix/src/Main.java", source: "gradle" }),
					problem({ path: "app/main/src/Main.java", source: "gradle" }),
				],
				updatedAt: null,
			}),
			getCheckRootsMockHandler({
				askAbove: 5,
				roots: ["app/main", "app/fix", "a", "b", "c", "d", "e"].map(root),
			}),
			getRunChecksMockHandler(async ({ request }) => {
				ran.push(await request.json());
				return { checks: [], counts: { errors: 0, warnings: 0, information: 0, hints: 0 } };
			}),
			getPinRepositoryMockHandler(async ({ request }) => {
				pinned.push(await request.json());
				return {
					activeDays: 14,
					items: [
						{
							repository: "app",
							worktree: "app/fix",
							pinned: true,
							active: true,
							lastActiveAt: null,
						},
						{ repository: "b", worktree: null, pinned: true, active: true, lastActiveAt: null },
					],
				};
			}),
		);
		renderWithQuery(<ProblemsPanel project={project} toolbar={null} onOpenFile={() => {}} />);
		// app, then its worktrees, then their files.
		expect(await screen.findByRole("button", { name: /^app/ })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /^fix/ })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /^main/ })).toBeInTheDocument();
		// a, c, d and e: not used, not pinned, nothing found (b is pinned).
		expect(await screen.findByText(/4 repositories not checked/)).toBeInTheDocument();

		await userEvent.click(screen.getByRole("button", { name: "Pin app/fix" }));
		expect(pinned).toEqual([{ repository: "app", worktree: "app/fix", pinned: true }]);
		expect(await screen.findByRole("button", { name: "Unpin app/fix" })).toBeInTheDocument();

		// Seven roots: the chooser opens with the active and pinned chosen.
		await userEvent.click(screen.getByRole("button", { name: "Check project" }));
		const chooser = await screen.findByRole("dialog");
		const boxes = screen.getAllByRole("checkbox");
		expect(boxes.filter((box) => (box as HTMLInputElement).checked)).toHaveLength(2);
		expect(chooser).toHaveTextContent("Check 2 repositories (2 roots)");
		await userEvent.click(screen.getByRole("button", { name: /^Check 2 repositories/ }));
		expect(ran).toEqual([{ checkouts: ["app/fix", "b"] }]);
	});

	it("checks everything at once with a few roots", async () => {
		const ran: unknown[] = [];
		mockServer.use(
			getGitRepositoriesMockHandler({ inside: true, repositories: [] }),
			getListRepositoryActivityMockHandler({ activeDays: 14, items: [] }),
			getListProblemsMockHandler({
				counts: { errors: 0, warnings: 0, information: 0, hints: 0 },
				items: [],
				updatedAt: null,
			}),
			getCheckRootsMockHandler({
				askAbove: 5,
				roots: [{ tool: "clippy", name: "cargo clippy", root: "", checkout: "" }],
			}),
			getRunChecksMockHandler(async ({ request }) => {
				ran.push(await request.json());
				return { checks: [], counts: { errors: 0, warnings: 0, information: 0, hints: 0 } };
			}),
		);
		renderWithQuery(<ProblemsPanel project={project} toolbar={null} onOpenFile={() => {}} />);
		await userEvent.click(await screen.findByRole("button", { name: "Check project" }));
		await vi.waitFor(() => expect(ran).toEqual([{}]));
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
	});
});
