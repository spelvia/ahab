import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { FolderPicker } from "@/modules/projects/files/components/folder-picker";
import {
	getHomeFolderMockHandler,
	getListFoldersMockHandler,
} from "@/shared/api/generated/projects-files/projects-files.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

describe("FolderPicker", () => {
	it("starts at home, walks into a folder and picks it", async () => {
		mockServer.use(
			getHomeFolderMockHandler({ path: "/home/me" }),
			getListFoldersMockHandler(({ request }) => {
				const path = new URL(request.url).searchParams.get("path") ?? "";
				const child = path === "/home/me" ? "code" : "ahab";
				return {
					path,
					parent: "/home",
					truncated: false,
					folders: [
						{ name: child, path: `${path}/${child}`, isGitRepo: child === "ahab", hidden: false },
					],
				};
			}),
		);
		const onPick = vi.fn<(machine: string, path: string) => void>();
		renderWithQuery(
			<FolderPicker
				machines={[{ id: "m1", name: "atlas", online: true }]}
				action="Open"
				onPick={onPick}
			/>,
		);

		await userEvent.click(await screen.findByRole("button", { name: /code/ }));
		expect(await screen.findByText("/home/me/code")).toBeInTheDocument();
		expect(screen.getByText("git")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Open: code" }));
		expect(onPick).toHaveBeenCalledWith("m1", "/home/me/code");
		// One machine: no machine to choose.
		expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
	});

	it("explains when no machine is online", () => {
		renderWithQuery(
			<FolderPicker
				machines={[{ id: "m1", name: "atlas", online: false }]}
				action="Open"
				onPick={() => {}}
			/>,
		);
		expect(screen.getByText(/No machine is online/)).toBeInTheDocument();
	});
});
