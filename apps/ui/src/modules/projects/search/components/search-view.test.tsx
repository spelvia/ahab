import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { SearchView } from "@/modules/projects/search/components/search-view";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

/** A streamed search's answer: newline-delimited frames. */
function frames(...lines: unknown[]) {
	return new HttpResponse(lines.map((line) => `${JSON.stringify(line)}\n`).join(""), {
		headers: { "Content-Type": "application/x-ndjson" },
	});
}

const project = {
	id: "w1",
	name: "app",
	availability: "available",
	mode: "readWrite",
} as Project;

describe("SearchView", () => {
	it("searches with its toggles and opens a result at its line", async () => {
		const asked: Record<string, unknown>[] = [];
		mockServer.use(
			http.post("*/api/v1/projects/w1/search/stream", async ({ request }) => {
				asked.push((await request.json()) as Record<string, unknown>);
				return frames(
					{
						type: "files",
						files: [
							{
								path: "src/main.rs",
								more: false,
								matches: [{ line: 3, preview: "    run_main();", ranges: [[8, 12]] }],
							},
						],
					},
					{ type: "done", total: 1, cut: null },
				);
			}),
		);
		const onOpen = vi.fn<(path: string, line: number) => void>();
		renderWithQuery(<SearchView project={project} onOpen={onOpen} />);

		await userEvent.click(screen.getByRole("button", { name: "Match case" }));
		await userEvent.type(screen.getByRole("textbox", { name: "Search" }), "main");
		expect(await screen.findByText("1 result in 1 file")).toBeInTheDocument();
		expect(asked.at(-1)).toMatchObject({ query: "main", matchCase: true, wholeWord: false });
		expect(screen.getByText("main", { selector: "mark" })).toBeInTheDocument();

		await userEvent.click(screen.getByText("3"));
		expect(onOpen).toHaveBeenCalledWith("src/main.rs", 3, false);
	});

	it("remembers the last search per project, and starts in a folder when asked", async () => {
		localStorage.clear();
		mockServer.use(
			http.post("*/api/v1/projects/w1/search/stream", () =>
				frames({ type: "done", total: 0, cut: null }),
			),
		);
		const first = renderWithQuery(<SearchView project={project} onOpen={() => {}} />);
		await userEvent.type(screen.getByRole("textbox", { name: "Search" }), "needle");
		await userEvent.click(screen.getByRole("button", { name: "Match case" }));
		await vi.waitFor(() => expect(localStorage.getItem("ahab.preferences.v1")).toContain("needle"));
		first.unmount();

		renderWithQuery(<SearchView project={project} folder="src/app" onOpen={() => {}} />);
		expect(screen.getByRole("textbox", { name: "Search" })).toHaveValue("needle");
		expect(screen.getByRole("button", { name: "Match case" })).toHaveAttribute(
			"aria-pressed",
			"true",
		);
		expect(screen.getByRole("textbox", { name: "Files to include" })).toHaveValue("src/app/**");
	});

	it("shows a bad pattern's error", async () => {
		mockServer.use(
			http.post("*/api/v1/projects/w1/search/stream", () =>
				HttpResponse.json(
					{ code: "validation_failed", title: "Invalid", detail: "regex parse error", status: 422 },
					{ status: 422, headers: { "Content-Type": "application/problem+json" } },
				),
			),
		);
		renderWithQuery(<SearchView project={project} onOpen={() => {}} />);
		await userEvent.type(screen.getByRole("textbox", { name: "Search" }), "(");
		expect(await screen.findByRole("alert")).toHaveTextContent("regex parse error");
	});
});
