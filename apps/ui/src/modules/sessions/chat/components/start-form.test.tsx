import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { type ChatStart, StartForm } from "@/modules/sessions/chat/components/start-form";
import { getListAgentsMockHandler } from "@/shared/api/generated/machines-agents/machines-agents.msw";
import { getListModelsMockHandler } from "@/shared/api/generated/sessions-chat/sessions-chat.msw";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const agent = (id: string, label: string, state: "ready" | "notSignedIn") => ({
	agent: id,
	label,
	state,
	checkedAt: "2026-11-14T09:00:00Z",
});

const efforts = [
	{ id: "low", description: "Quick" },
	{ id: "high", description: "Thorough" },
];

describe("StartForm", () => {
	it("lists commands before starting and sends the selected skill as the first message", async () => {
		mockServer.use(
			getListAgentsMockHandler({ items: [agent("codex", "Codex", "ready")] }),
			getListModelsMockHandler({ fetchedAt: "2026-11-16T10:00:00Z", refreshing: false, items: [] }),
			http.get("*/api/v1/projects/w1/agents/codex/commands", () =>
				HttpResponse.json([
					{ id: "project/review", kind: "skill", name: "review", description: "Review changes" },
				]),
			),
		);
		const onStart = vi.fn<(start: ChatStart) => void>();
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" onStart={onStart} />,
		);
		await screen.findByRole("button", { name: "Agent: Codex" });
		await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "/");
		await userEvent.click(await screen.findByRole("button", { name: /Review changes/ }));
		expect(onStart).not.toHaveBeenCalled();
		await userEvent.keyboard("my files{Enter}");
		expect(onStart).toHaveBeenCalledWith(
			expect.objectContaining({
				message: "/review my files",
				command: { id: "project/review", kind: "skill" },
			}),
		);
	});

	it("offers ready agents, their models and efforts, and starts with the choice", async () => {
		mockServer.use(
			getListAgentsMockHandler({
				items: [
					agent("claude-code", "Claude Code", "ready"),
					agent("codex", "Codex", "notSignedIn"),
					agent("pi", "Pi", "ready"),
				],
			}),
			getListModelsMockHandler({
				fetchedAt: "2026-11-16T10:00:00Z",
				refreshing: false,
				items: [
					{
						id: "default",
						label: "Default",
						description: "",
						isDefault: true,
						efforts,
						defaultEffort: "low",
					},
					{
						id: "opus",
						label: "Opus",
						description: "Most capable",
						isDefault: false,
						efforts,
						defaultEffort: null,
					},
				],
			}),
		);
		const onStart = vi.fn<(start: ChatStart) => void>();
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" onStart={onStart} />,
		);

		await userEvent.click(await screen.findByRole("button", { name: "Agent: Claude Code" }));
		expect(await screen.findByRole("menuitem", { name: /Pi/ })).toBeInTheDocument();
		expect(screen.queryByRole("menuitem", { name: /Codex/ })).not.toBeInTheDocument();
		await userEvent.keyboard("{Escape}");

		await userEvent.click(await screen.findByRole("button", { name: "Model: Default" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: /Opus/ }));
		await userEvent.click(screen.getByRole("button", { name: "Reasoning: Low" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: /High/ }));

		await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "Fix the test{Enter}");
		expect(onStart).toHaveBeenCalledWith({
			agent: "claude-code",
			message: "Fix the test",
			model: "opus",
			effort: "high",
			mode: "ask",
			attachments: [],
		});
	});

	it("starts in the chosen permission mode", async () => {
		mockServer.use(
			getListAgentsMockHandler({ items: [agent("claude-code", "Claude Code", "ready")] }),
			getListModelsMockHandler({ fetchedAt: "2026-11-16T10:00:00Z", refreshing: false, items: [] }),
		);
		const onStart = vi.fn<(start: ChatStart) => void>();
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" onStart={onStart} />,
		);
		await userEvent.click(await screen.findByRole("button", { name: "Permissions: Ask" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: /Accept edits/ }));
		await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "Go{Enter}");
		expect(onStart).toHaveBeenCalledWith(expect.objectContaining({ mode: "acceptEdits" }));
	});

	it("starts with the agent's saved defaults, and saves a new choice", async () => {
		const saved: unknown[] = [];
		let stored = {
			agent: "claude-code",
			model: "opus",
			effort: "low",
			mode: "bypass",
			updatedAt: "2026-11-15T09:00:00Z",
		};
		mockServer.use(
			getListAgentsMockHandler({ items: [agent("claude-code", "Claude Code", "ready")] }),
			getListModelsMockHandler({
				fetchedAt: "2026-11-16T10:00:00Z",
				refreshing: false,
				items: [
					{ id: "sonnet", label: "Sonnet", description: "", isDefault: true, efforts },
					{ id: "opus", label: "Opus", description: "", isDefault: false, efforts },
				],
			}),
			// The server's defaults, as saved.
			http.get("*/api/v1/agent-defaults", () => HttpResponse.json({ items: [stored] })),
			http.put("*/api/v1/agent-defaults/:agent", async ({ request }) => {
				const body = (await request.json()) as Record<string, string>;
				saved.push(body);
				stored = { ...stored, ...body };
				return HttpResponse.json(stored);
			}),
		);
		const onStart = vi.fn<(start: ChatStart) => void>();
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" onStart={onStart} />,
		);
		expect(await screen.findByRole("button", { name: "Model: Opus" })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Reasoning: Low" })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Permissions: Bypass" })).toBeInTheDocument();

		await userEvent.click(screen.getByRole("button", { name: "Reasoning: Low" }));
		await userEvent.click(await screen.findByRole("menuitem", { name: /High/ }));
		await waitFor(() => expect(saved).toEqual([{ model: "opus", effort: "high", mode: "bypass" }]));
		expect(await screen.findByRole("button", { name: "Reasoning: High" })).toBeInTheDocument();
	});

	it("uploads attached files and starts with them, even without text", async () => {
		mockServer.use(
			getListAgentsMockHandler({ items: [agent("claude-code", "Claude Code", "ready")] }),
			getListModelsMockHandler({ fetchedAt: "2026-11-16T10:00:00Z", refreshing: false, items: [] }),
			http.post("*/api/v1/projects/w1/attachments", ({ request }) => {
				const name = new URL(request.url).searchParams.get("name") ?? "";
				return HttpResponse.json(
					{ id: "a1", name, path: `/data/a1/${name}`, contentType: "text/plain", size: 5 },
					{ status: 201 },
				);
			}),
		);
		const onStart = vi.fn<(start: ChatStart) => void>();
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" onStart={onStart} />,
		);
		await screen.findByRole("button", { name: "Agent: Claude Code" });
		const send = screen.getByRole("button", { name: "Send" });
		expect(send).toBeDisabled();

		const input = document.querySelector<HTMLInputElement>('input[type="file"]');
		if (!input) throw new Error("no file input");
		await userEvent.upload(input, new File(["hello"], "notes.txt", { type: "text/plain" }));
		const list = screen.getByRole("list", { name: "Attached files" });
		expect(within(list).getByText("notes.txt")).toBeInTheDocument();
		await waitFor(() => expect(send).toBeEnabled());
		await userEvent.click(send);
		expect(onStart).toHaveBeenCalledWith(
			expect.objectContaining({ message: "", attachments: ["a1"] }),
		);
		expect(screen.queryByRole("list", { name: "Attached files" })).not.toBeInTheDocument();
	});

	it("says when the machine is offline", () => {
		mockServer.use(getListAgentsMockHandler({ items: [] }));
		renderWithQuery(
			<StartForm projectId="w1" machineId="m1" machineName="atlas" offline onStart={() => {}} />,
		);
		expect(screen.getByRole("textbox", { name: "Message" })).toBeDisabled();
		expect(screen.getByPlaceholderText("atlas is offline.")).toBeInTheDocument();
	});
});
