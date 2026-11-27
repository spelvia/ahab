import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { FileView } from "@/modules/projects/files/components/file-view";
import { getReadFileMockHandler } from "@/shared/api/generated/projects-files/projects-files.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const file = (path: string, content: string) =>
	getReadFileMockHandler({
		path,
		binary: false,
		content,
		size: content.length,
		modifiedAt: null,
	});

describe("FileView", () => {
	it("shows markdown rendered, as its source, or both", async () => {
		window.localStorage.clear();
		mockServer.use(file("README.md", "# Ahab\n\nSome **bold** text.\n"));
		renderWithQuery(<FileView projectId="w1" projectName="demo" path="README.md" />);

		// Rendered first.
		expect(await screen.findByRole("heading", { name: "Ahab" })).toBeInTheDocument();
		expect(screen.queryByText("# Ahab")).not.toBeInTheDocument();

		await userEvent.click(screen.getByRole("radio", { name: "Source" }));
		expect(await screen.findByText("# Ahab")).toBeInTheDocument();
		expect(screen.queryByRole("heading", { name: "Ahab" })).not.toBeInTheDocument();

		await userEvent.click(screen.getByRole("radio", { name: "Side by side" }));
		expect(await screen.findByText("# Ahab")).toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "Ahab" })).toBeInTheDocument();
	});

	it("remembers the choice for the next markdown file, and leaves other files alone", async () => {
		window.localStorage.clear();
		mockServer.use(file("notes.md", "# Notes\n"));
		const { unmount } = renderWithQuery(
			<FileView projectId="w1" projectName="demo" path="notes.md" />,
		);
		await userEvent.click(await screen.findByRole("radio", { name: "Source" }));
		unmount();
		renderWithQuery(<FileView projectId="w1" projectName="demo" path="notes.md" />);
		expect(await screen.findByText("# Notes")).toBeInTheDocument();

		mockServer.use(file("main.rs", "fn main() {}\n"));
		renderWithQuery(<FileView projectId="w1" projectName="demo" path="main.rs" />);
		expect(await screen.findByText("fn main() {}")).toBeInTheDocument();
		expect(screen.getAllByRole("radiogroup")).toHaveLength(1);
	});

	it("opens linked files of the project and scrolls to linked headings", async () => {
		window.localStorage.clear();
		mockServer.use(
			file(
				"rfcs/004.md",
				"# Sessions\n\nSee [the daemon](002-daemon.md#messages), [below](#open-questions) and [the web](https://example.com).\n\n## Open questions\n",
			),
		);
		const scrolled = vi.fn<() => void>();
		Element.prototype.scrollIntoView = scrolled;
		const onOpenFile = vi.fn<(path: string, anchor?: string) => void>();
		renderWithQuery(
			<FileView projectId="w1" projectName="demo" path="rfcs/004.md" onOpenFile={onOpenFile} />,
		);
		await userEvent.click(await screen.findByRole("link", { name: "the daemon" }));
		expect(onOpenFile).toHaveBeenCalledWith("rfcs/002-daemon.md", "messages");

		await userEvent.click(screen.getByRole("link", { name: "below" }));
		expect(scrolled).toHaveBeenCalled();
		expect(screen.getByRole("heading", { name: "Open questions" })).toHaveAttribute(
			"id",
			"open-questions",
		);
		expect(screen.getByRole("link", { name: "the web" })).toHaveAttribute("target", "_blank");
	});
});
