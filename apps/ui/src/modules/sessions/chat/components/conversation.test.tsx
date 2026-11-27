import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";

import { ChatRoot } from "@/modules/sessions/chat/components/chat-root";
import { ConversationView } from "@/modules/sessions/chat/components/conversation";
import type { Entry } from "@/modules/sessions/chat/model";
import { renderWithQuery } from "@/testing/render";

afterEach(async () => {
	vi.useRealTimers();
	await vi.dynamicImportSettled();
});

function message(text: string, done = false) {
	const entry: Entry = {
		key: "item:answer",
		kind: "item",
		item: { id: "answer", type: "agentMessage", text: done ? text : "" },
		done,
		streamed: done ? "" : text,
	};
	return <ConversationView entries={[entry]} />;
}

it("coalesces streamed renders and renders the finished Markdown immediately", () => {
	vi.useFakeTimers();
	const view = render(message("First"));
	view.rerender(message("Second"));
	view.rerender(message("Third"));
	expect(screen.getByText("First")).toBeInTheDocument();
	act(() => vi.advanceTimersByTime(80));
	expect(screen.queryByText("Second")).toBeNull();
	expect(screen.getByText("Third")).toBeInTheDocument();
	view.rerender(message("**Final**", true));
	expect(screen.getByText("Final").tagName).toBe("STRONG");
});

it("shows a request where it was asked, without the tool call that asked it, and declined ones dimly", async () => {
	const entries: Entry[] = [
		{
			key: "item:toolu_1",
			kind: "item",
			done: false,
			streamed: "",
			item: {
				type: "toolCall",
				id: "toolu_1",
				status: "inProgress",
				detail: { kind: "other", name: "AskUserQuestion", input: {} },
			},
		},
		{
			key: "request:2",
			kind: "request",
			request: {
				id: "q1",
				native: {},
				kind: {
					type: "question",
					toolCallId: "toolu_1",
					questions: [
						{
							id: "q",
							question: "Which library?",
							options: [{ label: "time" }],
							multiSelect: false,
							allowOther: false,
							secret: false,
						},
					],
				},
			},
		},
		{ key: "declined:3", kind: "declined", method: "mcpServer/elicitation", summary: null },
	];
	renderWithQuery(
		<ConversationView
			entries={entries}
			requests={{ chatId: "c1", agent: "Claude Code", open: new Set(["q1"]) }}
		/>,
	);
	expect(screen.getByRole("region", { name: "Which library?" })).toBeInTheDocument();
	expect(screen.queryByText("AskUserQuestion")).toBeNull();
	expect(screen.getByText("Declined automatically: mcpServer/elicitation")).toBeInTheDocument();
});

it("shows an MCP call's server, tool, arguments and result", async () => {
	const entries: Entry[] = [
		{
			key: "item:m1",
			kind: "item",
			done: true,
			streamed: "",
			item: {
				type: "toolCall",
				id: "m1",
				status: "completed",
				detail: {
					kind: "mcpCall",
					server: "github",
					tool: "search_issues",
					arguments: { query: "flaky", limit: 5 },
					result: {
						content: [{ type: "text", text: "3 issues" }],
						structuredContent: { total: 3 },
					},
				},
			},
		},
	];
	renderWithQuery(<ConversationView entries={entries} />);
	expect(screen.getByText("search_issues")).toBeInTheDocument();
	await userEvent.click(screen.getByRole("button", { name: /search_issues/ }));
	expect(screen.getByText("query")).toBeInTheDocument();
	expect(screen.getByText("flaky")).toBeInTheDocument();
	expect(screen.getByText("3 issues")).toBeInTheDocument();
	expect(screen.getByText(/"total": 3/)).toBeInTheDocument();
});

function tool(
	id: string,
	detail: import("@/modules/sessions/chat/model").ToolCallItem["detail"],
): Entry {
	return {
		key: "item:" + id,
		kind: "item",
		done: true,
		streamed: "",
		item: { type: "toolCall", id, status: "completed", detail },
	};
}

it("groups completed reads and opens the reported location only inside the project", async () => {
	const open = vi.fn<(path: string, line?: number) => void>();
	render(
		<ChatRoot value="/project">
			<ConversationView
				entries={[
					tool("a", {
						kind: "other",
						name: "Read",
						input: { file_path: "/project/a #1.ts", offset: 8 },
					}),
					tool("b", { kind: "other", name: "Read", input: { file_path: "/outside/b.ts" } }),
				]}
				onOpenFile={open}
			/>
		</ChatRoot>,
	);
	await userEvent.click(screen.getByRole("button", { name: "Read 2 files" }));
	await userEvent.click(screen.getByRole("button", { name: "Read · /project/a #1.ts" }));
	await userEvent.click(screen.getByRole("button", { name: "a #1.ts:8" }));
	expect(open).toHaveBeenCalledWith("a #1.ts", 8);
	await userEvent.click(screen.getByRole("button", { name: "Read · /outside/b.ts" }));
	expect(screen.queryByRole("button", { name: "/outside/b.ts" })).toBeNull();
});

it("preserves historical references and attachment IDs on an edited resend", async () => {
	const send = vi
		.fn<(text: string, files: string[]) => Promise<void>>()
		.mockResolvedValue(undefined);
	const id = "01a1db75-0000-7000-8000-000000000001";
	render(
		<ConversationView
			entries={[
				{
					key: "u",
					kind: "item",
					done: true,
					streamed: "",
					item: {
						type: "userMessage",
						id: "u",
						content: [
							{
								type: "text",
								text:
									"Fix it\n\nContext: file.ts (the whole file)\n\n[Attached files]\n/tmp/" +
									id +
									"/sample.png",
							},
						],
					},
				},
			]}
			onResend={send}
		/>,
	);
	await userEvent.click(screen.getByRole("button", { name: "Edit and resend" }));
	const field = screen.getByRole("textbox", { name: "Edit message" });
	await userEvent.clear(field);
	await userEvent.type(field, "Check it");
	await userEvent.click(screen.getByRole("button", { name: "Send again" }));
	expect(send).toHaveBeenCalledWith("Check it\n\nContext: file.ts (the whole file)", [id]);
});

it("renders created source lines and reported diff locations", async () => {
	const open = vi.fn<(path: string, line?: number) => void>();
	render(
		<ConversationView
			onOpenFile={open}
			entries={[
				tool("files", {
					kind: "fileChange",
					changes: [
						{ path: "app.ts", kind: { type: "add" }, diff: "+function main() {\n+}" },
						{ path: "a.rs", kind: { type: "update" }, diff: "@@ -3,1 +9,1 @@\n-old\n+new" },
					],
				}),
			]}
		/>,
	);
	await userEvent.click(screen.getByRole("button", { name: /2 files/i }));
	await userEvent.click(screen.getByRole("button", { name: "app.ts:2" }));
	await userEvent.click(screen.getByRole("button", { name: "a.rs:9" }));
	expect(open.mock.calls).toEqual([
		["app.ts", 2],
		["a.rs", 9],
	]);
});

it("shows a wrapped command in full as one shell block, and the wrapper on request", async () => {
	const command = `/bin/zsh -lc 'set -e\ncat > tools/seed.py <<'\\''EOF'\\''\nprint("it'\\''s")\nEOF\npython3 tools/seed.py'`;
	render(
		<ConversationView
			entries={[
				tool("cmd", {
					kind: "command",
					command,
					cwd: "/project",
					output: '{"ok": true}',
					exitCode: 0,
				} as never),
			]}
		/>,
	);
	const row = screen.getByRole("button", { name: "cat > tools/seed.py <<'EOF' …" });
	expect(row.querySelector("[title]")?.getAttribute("title")).toContain(`print("it's")`);
	await userEvent.click(row);
	const text = screen.getByRole("region", { name: "Command" });
	expect(text).toHaveTextContent("set -e");
	expect(text).toHaveTextContent("python3 tools/seed.py");
	// The heredoc stays inside the command, not in a box of its own.
	expect(text).toHaveTextContent(`print("it's")`);
	expect(text.querySelector("pre, div[class*='border']")).toBeNull();
	expect(screen.getByText("Command", { selector: "span" })).toBeInTheDocument();
	expect(screen.getByText("Output", { selector: "span" })).toBeInTheDocument();
	expect(screen.getByRole("button", { name: "Copy command" })).toBeInTheDocument();
	expect(screen.getByRole("button", { name: "Copy output" })).toBeInTheDocument();
	const raw = screen.getByText("Run through /bin/zsh -lc").closest("details");
	expect(raw).not.toHaveAttribute("open");
	expect(raw?.querySelector("pre")?.textContent).toBe(command);
});

it("caps a created file's lines and keeps its line numbers out of the tab order", async () => {
	const body = Array.from({ length: 450 }, (_, i) => `+line ${i}`).join("\n");
	render(
		<ConversationView
			onOpenFile={() => {}}
			entries={[
				tool("big", {
					kind: "fileChange",
					changes: [{ path: "big.ts", kind: { type: "add" }, diff: body }],
				}),
			]}
		/>,
	);
	await userEvent.click(screen.getByRole("button", { name: /big\.ts/ }));
	expect(screen.getByRole("button", { name: "big.ts:400" })).toHaveAttribute("tabindex", "-1");
	expect(screen.queryByRole("button", { name: "big.ts:401" })).toBeNull();
	await userEvent.click(screen.getByRole("button", { name: "Show all 450 lines" }));
	expect(screen.getByRole("button", { name: "big.ts:450" })).toBeInTheDocument();
});
