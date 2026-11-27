import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { DiffView } from "@/modules/projects/git/components/diff-view";
import { getGitDiffMockHandler } from "@/shared/api/generated/projects-git/projects-git.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const diff = {
	path: "a.txt",
	old: "one\ntwo\nthree\nfour\n",
	new: "one\n2\nthree\nfour\nfive\n",
	binary: false,
	tooLarge: false,
	partial: true,
	index: "abc123",
};

function staged() {
	const bodies: unknown[] = [];
	mockServer.use(
		http.post("*/api/v1/projects/w1/git/stage-part", async ({ request }) => {
			bodies.push(await request.json());
			return HttpResponse.json({ output: "" });
		}),
	);
	return bodies;
}

describe("DiffView", () => {
	it("stages one hunk of a file", async () => {
		mockServer.use(getGitDiffMockHandler(diff));
		const bodies = staged();
		renderWithQuery(<DiffView projectId="w1" path="a.txt" group="changes" />);
		const [, second] = await screen.findAllByRole("button", { name: "Stage hunk" });
		await userEvent.click(second as HTMLElement);
		await vi.waitFor(() =>
			expect(bodies).toEqual([
				{ path: "a.txt", content: "one\ntwo\nthree\nfour\nfive\n", index: "abc123" },
			]),
		);
	});

	it("stages chosen lines, and unstages them from staged changes", async () => {
		mockServer.use(getGitDiffMockHandler(diff));
		const bodies = staged();
		const { unmount } = renderWithQuery(<DiffView projectId="w1" path="a.txt" group="changes" />);
		await userEvent.click(await screen.findByRole("button", { name: "Choose line 5" }));
		await userEvent.click(screen.getByRole("button", { name: "Stage 1 line" }));
		await vi.waitFor(() => expect(bodies).toHaveLength(1));
		expect(bodies[0]).toMatchObject({ content: "one\ntwo\nthree\nfour\nfive\n" });
		unmount();

		renderWithQuery(<DiffView projectId="w1" path="a.txt" group="staged" />);
		await userEvent.click(await screen.findByRole("button", { name: "Choose line 5" }));
		await userEvent.click(screen.getByRole("button", { name: "Unstage 1 line" }));
		// Unstaging the added line keeps every other staged change.
		await vi.waitFor(() => expect(bodies).toHaveLength(2));
		expect(bodies[1]).toMatchObject({ content: "one\n2\nthree\nfour\n" });
	});

	it("offers no staging for a commit's diff", async () => {
		mockServer.use(getGitDiffMockHandler({ ...diff, partial: false, index: undefined }));
		renderWithQuery(<DiffView projectId="w1" path="a.txt" group="commit" commit="abc" />);
		await screen.findByText("five");
		expect(screen.queryByRole("button", { name: "Stage hunk" })).not.toBeInTheDocument();
	});
});
