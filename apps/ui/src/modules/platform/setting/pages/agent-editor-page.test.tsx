import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, it } from "vitest";

import { AgentEditorPage } from "@/modules/platform/setting/pages/agent-editor-page";
import { EditorSettingsPage } from "@/modules/platform/setting/pages/editor-settings-page";
import { changeAppearance } from "@/shared/appearance";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const kept = () => JSON.parse(localStorage.getItem("ahab.preferences.v1") ?? "{}").appearance;

const field = (name: string, kind: "boolean" | "choice") => ({
	name,
	description: `${name} described`,
	perProject: false,
	...(kind === "boolean"
		? { kind, default: false }
		: { kind, default: "queue", options: ["queue", "interrupt"] }),
});

function serve() {
	mockServer.use(
		http.get("*/api/v1/settings/editor", () =>
			HttpResponse.json({
				section: "editor",
				fields: [
					field("vim", "boolean"),
					field("chatWhileWorking", "choice"),
					field("chatVim", "boolean"),
				],
				values: { vim: false, chatWhileWorking: "queue", chatVim: false },
				server: {},
			}),
		),
	);
}

it("keeps the transcript and session usage choices on this device", async () => {
	changeAppearance({ expandThinking: false, expandTools: false, showCost: true, showTokens: true });
	serve();
	renderWithQuery(<AgentEditorPage />);
	await userEvent.click(screen.getByRole("switch", { name: "Show thinking expanded" }));
	await userEvent.click(
		screen.getByRole("switch", { name: "Show commands and tool calls expanded" }),
	);
	await userEvent.click(screen.getByRole("switch", { name: "Show session cost" }));
	await userEvent.click(screen.getByRole("switch", { name: "Show token usage" }));
	expect(kept()).toMatchObject({
		expandThinking: true,
		expandTools: true,
		showCost: false,
		showTokens: false,
	});
});

it("shows the composer's fields here, and only here", async () => {
	serve();
	const page = renderWithQuery(<AgentEditorPage />);
	expect(await screen.findByText("Vim key bindings in the chat composer")).toBeInTheDocument();
	expect(screen.getByText("Messages sent while the agent works")).toBeInTheDocument();
	expect(screen.queryByText("Vim key bindings")).not.toBeInTheDocument();
	page.unmount();
	renderWithQuery(<EditorSettingsPage />);
	expect(await screen.findByText("Vim key bindings")).toBeInTheDocument();
	expect(screen.queryByText("Vim key bindings in the chat composer")).not.toBeInTheDocument();
});
