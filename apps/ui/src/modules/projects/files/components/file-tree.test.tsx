import { createEvent, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { FileTree } from "@/modules/projects/files/components/file-tree";
import type { FileEntry } from "@/shared/api/generated/ahab.schemas";
import {
	getDeleteFileMockHandler,
	getListFilesMockHandler,
	getMoveFileMockHandler,
	getRenameFileMockHandler,
} from "@/shared/api/generated/projects-files/projects-files.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const entry = (path: string, kind: FileEntry["kind"]): FileEntry => ({
	name: path.split("/").at(-1) ?? path,
	path,
	kind,
	symlink: false,
	hidden: path.startsWith("."),
});

// Open folders are remembered between visits; each test starts with none.
beforeEach(() => window.localStorage.clear());

describe("FileTree", () => {
	it("lists a project, opens folders as asked and files on a click", async () => {
		mockServer.use(
			getListFilesMockHandler(({ request }) => {
				const path = new URL(request.url).searchParams.get("path") ?? "";
				return path === "src"
					? { path, truncated: false, entries: [entry("src/main.rs", "file")] }
					: {
							path,
							truncated: false,
							entries: [entry("src", "dir"), entry("README.md", "file")],
						};
			}),
		);
		const onOpen = vi.fn<(path: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={onOpen} />);

		await userEvent.click(await screen.findByRole("treeitem", { name: "README.md" }));
		expect(onOpen).toHaveBeenCalledWith("README.md");
		await userEvent.dblClick(screen.getByRole("treeitem", { name: "README.md" }));
		expect(onOpen).toHaveBeenLastCalledWith("README.md", true);

		expect(screen.queryByText("main.rs")).not.toBeInTheDocument();
		await userEvent.click(screen.getByRole("treeitem", { name: "src" }));
		await userEvent.click(await screen.findByRole("treeitem", { name: "main.rs" }));
		expect(onOpen).toHaveBeenCalledWith("src/main.rs");
	});

	it("refreshes the root and open folders in a read-only project", async () => {
		let updated = false;
		mockServer.use(
			getListFilesMockHandler(({ request }) => {
				const path = new URL(request.url).searchParams.get("path") ?? "";
				const entries = path ? [entry("src/main.rs", "file")] : [entry("src", "dir")];
				if (updated) entries.push(entry(path ? "src/extra.rs" : "notes.md", "file"));
				return { path, entries, truncated: false };
			}),
		);
		renderWithQuery(<FileTree projectId="w1" readOnly onOpen={() => {}} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "src" }));
		await screen.findByText("main.rs");
		updated = true;
		await userEvent.click(screen.getByRole("button", { name: "Refresh file explorer" }));
		expect(await screen.findByText("extra.rs")).toBeInTheDocument();
		expect(await screen.findByText("notes.md")).toBeInTheDocument();
	});

	it("right-clicking a file or folder opens its own menu, and copies its paths", async () => {
		mockServer.use(
			getListFilesMockHandler(({ request }) => {
				const path = new URL(request.url).searchParams.get("path") ?? "";
				return {
					path,
					truncated: false,
					entries: [entry("src", "dir"), entry("README.md", "file")],
				};
			}),
		);
		const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue();
		Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
		const onOpenBeside = vi.fn<(path: string) => void>();
		renderWithQuery(
			<FileTree projectId="w1" root="/work/app/" onOpen={() => {}} onOpenBeside={onOpenBeside} />,
		);

		fireEvent.contextMenu(await screen.findByRole("treeitem", { name: "README.md" }));
		for (const item of [
			"Open",
			"Open to the side",
			"Copy path",
			"Copy relative path",
			"Rename…",
			"Duplicate",
			"Delete…",
		])
			expect(screen.getByRole("menuitem", { name: item })).toBeInTheDocument();
		await userEvent.click(screen.getByRole("menuitem", { name: "Copy path" }));
		expect(writeText).toHaveBeenCalledWith("/work/app/README.md");

		fireEvent.contextMenu(screen.getByRole("treeitem", { name: "src" }));
		expect(await screen.findByRole("menuitem", { name: "New folder…" })).toBeInTheDocument();
		await userEvent.click(screen.getByRole("menuitem", { name: "Copy relative path" }));
		expect(writeText).toHaveBeenLastCalledWith("src");
	});

	it("says when the machine is offline", async () => {
		mockServer.use(
			http.get("*/api/v1/projects/:id/files", () =>
				HttpResponse.json(
					{ code: "machine_offline", title: "Unavailable", status: 503 },
					{ status: 503 },
				),
			),
		);
		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} />);
		expect(await screen.findByText("The machine is offline.")).toBeInTheDocument();
	});
});

/** A drag's data, as a browser keeps it between dragstart and drop. */
function dataTransfer() {
	const data = new Map<string, string>();
	return {
		setData: (type: string, value: string) => data.set(type, value),
		getData: (type: string) => data.get(type) ?? "",
		get types() {
			return [...data.keys()];
		},
		dropEffect: "none",
		effectAllowed: "all",
	};
}

/** jsdom has no DragEvent: ⌥ (Alt) held is set by hand. */
function withAlt(event: Event) {
	Object.defineProperty(event, "altKey", { value: true });
	return event;
}

/** A project with `src/`, `docs/` (holding a README.md) and README.md. */
function listing() {
	return getListFilesMockHandler(({ request }) => {
		const path = new URL(request.url).searchParams.get("path") ?? "";
		if (path === "docs")
			return { path, truncated: false, entries: [entry("docs/README.md", "file")] };
		if (path === "src") return { path, truncated: false, entries: [entry("src/main.rs", "file")] };
		return {
			path,
			truncated: false,
			entries: [entry("docs", "dir"), entry("src", "dir"), entry("README.md", "file")],
		};
	});
}

describe("dragging in the FileTree", () => {
	it("moves a file into a folder, keeps both on a clash, follows it and undoes it", async () => {
		const renames: { from: string; to: string }[] = [];
		mockServer.use(
			listing(),
			http.post("*/api/v1/projects/:id/files/rename", async ({ request }) => {
				const body = (await request.json()) as { from: string; to: string };
				renames.push(body);
				if (body.to === "docs/README.md")
					return HttpResponse.json(
						{ code: "already_exists", title: "Conflict", status: 409 },
						{ status: 409 },
					);
				return HttpResponse.json({ path: body.to });
			}),
		);
		const onRenamed = vi.fn<(from: string, to: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} onRenamed={onRenamed} />);
		const file = await screen.findByRole("treeitem", { name: "README.md" });
		const docs = screen.getByRole("treeitem", { name: "docs" });
		const drag = dataTransfer();
		fireEvent.dragStart(file, { dataTransfer: drag });
		fireEvent.dragOver(docs, { dataTransfer: drag });
		expect(drag.dropEffect).toBe("move");
		fireEvent.drop(docs, { dataTransfer: drag });

		await userEvent.click(await screen.findByRole("button", { name: "Keep both" }));
		await waitFor(() => expect(onRenamed).toHaveBeenCalledWith("README.md", "docs/README copy.md"));
		expect(renames).toEqual([
			{ from: "README.md", to: "docs/README.md" },
			{ from: "README.md", to: "docs/README copy.md" },
		]);

		fireEvent.keyDown(screen.getByRole("tree"), { key: "z", metaKey: true });
		await waitFor(() =>
			expect(onRenamed).toHaveBeenLastCalledWith("docs/README copy.md", "README.md"),
		);
	});

	it("refuses a folder into itself, and every drop in a read-only project", async () => {
		const asked = vi.fn<() => void>();
		mockServer.use(
			listing(),
			http.post("*/api/v1/projects/:id/files/rename", () => {
				asked();
				return HttpResponse.json({ path: "x" });
			}),
		);
		const view = renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} />);
		const src = await screen.findByRole("treeitem", { name: "src" });
		const drag = dataTransfer();
		fireEvent.dragStart(src, { dataTransfer: drag });
		fireEvent.dragOver(src, { dataTransfer: drag });
		expect(drag.dropEffect).toBe("none");
		fireEvent.drop(src, { dataTransfer: drag });
		view.unmount();

		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} readOnly />);
		const file = await screen.findByRole("treeitem", { name: "README.md" });
		const again = dataTransfer();
		fireEvent.dragStart(file, { dataTransfer: again });
		fireEvent.dragOver(screen.getByRole("treeitem", { name: "docs" }), { dataTransfer: again });
		expect(again.dropEffect).toBe("none");
		fireEvent.drop(screen.getByRole("treeitem", { name: "docs" }), { dataTransfer: again });
		await new Promise((resolve) => setTimeout(resolve, 50));
		expect(asked).not.toHaveBeenCalled();
	});

	it("copies with ⌥ held, several chosen with ⌘-click, onto the project's folder", async () => {
		const moves: unknown[] = [];
		mockServer.use(
			listing(),
			getMoveFileMockHandler(async ({ request }) => {
				const body = (await request.json()) as { to: string };
				moves.push(body);
				return { path: body.to };
			}),
		);
		const onRenamed = vi.fn<(from: string, to: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} onRenamed={onRenamed} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "docs" }));
		const nested = await screen.findByTitle("docs/README.md");
		// Opening docs chose it; ⌘-click lets it go again.
		fireEvent.click(screen.getByRole("treeitem", { name: "docs" }), { metaKey: true });
		fireEvent.click(nested, { metaKey: true });
		fireEvent.click(screen.getByRole("treeitem", { name: "src" }), { ctrlKey: true });
		expect(nested).toHaveAttribute("aria-selected", "true");
		const drag = dataTransfer();
		fireEvent.dragStart(screen.getByRole("treeitem", { name: "src" }), { dataTransfer: drag });
		expect(JSON.parse(drag.getData("application/x-ahab-files"))).toEqual({
			projectId: "w1",
			paths: ["docs/README.md", "src"],
			folders: ["src"],
		});
		const tree = screen.getByRole("tree");
		fireEvent(tree, withAlt(createEvent.dragOver(tree, { dataTransfer: drag })));
		expect(drag.dropEffect).toBe("copy");
		fireEvent(tree, withAlt(createEvent.drop(tree, { dataTransfer: drag })));
		await waitFor(() => expect(moves).toHaveLength(1));
		// src is in the project's folder already; only the README is copied.
		expect(moves).toEqual([
			{ from: "docs/README.md", to: "README.md", copy: true, replace: false },
		]);
		expect(onRenamed).not.toHaveBeenCalled();
	});
});

/** Whether each row (by its title, the path) is chosen. */
function chosen() {
	return Object.fromEntries(
		screen
			.getAllByRole("treeitem")
			.map((row) => [row.getAttribute("data-path"), row.getAttribute("aria-selected") === "true"]),
	);
}

describe("choosing in the FileTree", () => {
	it("chooses a range with Shift-click in the order shown, and adds or drops with ⌘-click", async () => {
		mockServer.use(listing());
		const onOpen = vi.fn<(path: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={onOpen} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "docs" }));
		await screen.findByTitle("docs/README.md");
		fireEvent.click(screen.getByTitle("README.md"), { shiftKey: true });
		expect(chosen()).toEqual({
			docs: true,
			"docs/README.md": true,
			src: true,
			"README.md": true,
		});
		expect(onOpen).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("treeitem", { name: "src" }), { ctrlKey: true });
		expect(chosen().src).toBe(false);
		// A plain click chooses one again, and opens it.
		await userEvent.click(screen.getByTitle("docs/README.md"));
		expect(chosen()).toEqual({
			docs: false,
			"docs/README.md": true,
			src: false,
			"README.md": false,
		});
		expect(onOpen).toHaveBeenCalledWith("docs/README.md");
		expect(screen.getByRole("tree")).toHaveAttribute("aria-multiselectable", "true");
	});

	it("moves with the keys, opens and closes folders, extends with Shift and jumps to typed names", async () => {
		mockServer.use(listing());
		const onOpen = vi.fn<(path: string, keep?: boolean) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={onOpen} />);
		const docs = await screen.findByRole("treeitem", { name: "docs" });
		// One row takes the focus: the first, until another is focused.
		expect(docs).toHaveAttribute("tabindex", "0");
		docs.focus();

		await userEvent.keyboard("{ArrowDown}");
		const src = screen.getByRole("treeitem", { name: "src" });
		expect(src).toHaveFocus();
		expect(src).toHaveAttribute("aria-selected", "true");
		expect(src).toHaveAttribute("aria-expanded", "false");
		await userEvent.keyboard("{ArrowRight}");
		expect(src).toHaveAttribute("aria-expanded", "true");
		await screen.findByTitle("src/main.rs");
		await userEvent.keyboard("{ArrowRight}");
		expect(screen.getByTitle("src/main.rs")).toHaveFocus();
		await userEvent.keyboard("{ArrowLeft}");
		expect(src).toHaveFocus();
		await userEvent.keyboard("{ArrowLeft}");
		expect(src).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByTitle("src/main.rs")).not.toBeInTheDocument();

		await userEvent.keyboard("{End}");
		expect(screen.getByTitle("README.md")).toHaveFocus();
		await userEvent.keyboard("{Home}");
		expect(docs).toHaveFocus();
		await userEvent.keyboard("r");
		expect(screen.getByTitle("README.md")).toHaveFocus();
		await new Promise((resolve) => setTimeout(resolve, 900));
		await userEvent.keyboard(" ");
		expect(onOpen).toHaveBeenLastCalledWith("README.md");
		await userEvent.keyboard("{Enter}");
		expect(onOpen).toHaveBeenLastCalledWith("README.md", true);

		await userEvent.keyboard("{Shift>}{ArrowUp}{/Shift}");
		expect(chosen()).toEqual({ docs: false, src: true, "README.md": true });
		await userEvent.keyboard("{Control>}a{/Control}");
		expect(chosen()).toEqual({ docs: true, src: true, "README.md": true });
		await userEvent.keyboard("{Escape}");
		expect(chosen()).toEqual({ docs: false, src: false, "README.md": false });

		await userEvent.keyboard("{F2}");
		expect(await screen.findByRole("dialog", { name: "Rename src" })).toBeInTheDocument();
	});

	it("deletes what is chosen after one question, and its windows close", async () => {
		const deleted: string[] = [];
		mockServer.use(
			listing(),
			getDeleteFileMockHandler(({ request }) => {
				deleted.push(new URL(request.url).searchParams.get("path") ?? "");
			}),
		);
		const onDeleted = vi.fn<(path: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} onDeleted={onDeleted} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "src" }));
		fireEvent.click(screen.getByTitle("README.md"), { shiftKey: true });

		fireEvent.contextMenu(screen.getByTitle("README.md"));
		expect(await screen.findByRole("menuitem", { name: "Delete 2 items…" })).toBeInTheDocument();
		expect(screen.queryByRole("menuitem", { name: "Rename…" })).not.toBeInTheDocument();
		await userEvent.keyboard("{Escape}");

		screen.getByTitle("README.md").focus();
		await userEvent.keyboard("{Delete}");
		expect(await screen.findByRole("alertdialog", { name: "Delete 2 items?" })).toHaveTextContent(
			"src and README.md, with everything in the folders",
		);
		await userEvent.click(screen.getByRole("button", { name: "Delete" }));
		await waitFor(() => expect(onDeleted).toHaveBeenCalledTimes(2));
		expect(deleted).toEqual(["src", "README.md"]);
		expect(onDeleted.mock.calls).toEqual([["src"], ["README.md"]]);
	});

	it("copies the paths of what is chosen, one a line", async () => {
		mockServer.use(listing());
		const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue();
		Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
		renderWithQuery(<FileTree projectId="w1" root="/work/app" onOpen={() => {}} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "docs" }));
		fireEvent.click(screen.getByTitle("README.md"), { metaKey: true });
		fireEvent.contextMenu(screen.getByTitle("README.md"));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Copy paths" }));
		expect(writeText).toHaveBeenCalledWith("/work/app/docs\n/work/app/README.md");
	});

	it("cuts and pastes into the focused folder, and copies beside itself in its own", async () => {
		const renames: unknown[] = [];
		const copies: unknown[] = [];
		mockServer.use(
			listing(),
			getRenameFileMockHandler(async ({ request }) => {
				const body = (await request.json()) as { from: string; to: string };
				renames.push(body);
				return { path: body.to };
			}),
			getMoveFileMockHandler(async ({ request }) => {
				const body = (await request.json()) as { to: string };
				copies.push(body);
				return { path: body.to };
			}),
		);
		const onRenamed = vi.fn<(from: string, to: string) => void>();
		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} onRenamed={onRenamed} />);
		await userEvent.click(await screen.findByTitle("README.md"));
		await userEvent.keyboard("{Control>}c{/Control}");
		await userEvent.keyboard("{Control>}v{/Control}");
		await waitFor(() =>
			expect(copies).toEqual([
				{ from: "README.md", to: "README copy.md", copy: true, replace: false },
			]),
		);

		await userEvent.keyboard("{Control>}x{/Control}");
		expect(screen.getByTitle("README.md")).toHaveClass("opacity-55");
		await userEvent.click(screen.getByRole("treeitem", { name: "src" }));
		fireEvent.contextMenu(screen.getByRole("treeitem", { name: "src" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: "Paste" }));
		await waitFor(() => expect(onRenamed).toHaveBeenCalledWith("README.md", "src/README.md"));
		expect(renames).toEqual([{ from: "README.md", to: "src/README.md" }]);
	});

	it("collapses every folder, and keeps the open ones for the next visit", async () => {
		mockServer.use(listing());
		const view = renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} />);
		await userEvent.click(await screen.findByRole("treeitem", { name: "docs" }));
		await screen.findByTitle("docs/README.md");
		view.unmount();

		renderWithQuery(<FileTree projectId="w1" onOpen={() => {}} />);
		expect(await screen.findByTitle("docs/README.md")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Collapse folders" }));
		expect(screen.queryByTitle("docs/README.md")).not.toBeInTheDocument();
	});

	it("reveals the file opened: its folders open, it is chosen", async () => {
		mockServer.use(listing());
		renderWithQuery(<FileTree projectId="w1" selected="src/main.rs" onOpen={() => {}} />);
		expect(await screen.findByTitle("src/main.rs")).toHaveAttribute("aria-selected", "true");
		expect(screen.getByRole("treeitem", { name: "src" })).toHaveAttribute("aria-expanded", "true");
	});
});
