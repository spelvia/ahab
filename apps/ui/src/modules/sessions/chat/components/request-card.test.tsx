import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { RequestCard } from "@/modules/sessions/chat/components/request-card";
import type { AgentRequest, RequestOutcome } from "@/modules/sessions/chat/model";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

/** Answers sent to the server, as `{ request, body }`. */
function answers() {
	const sent: unknown[] = [];
	mockServer.use(
		http.post("*/api/v1/chats/:id/requests/:request", async ({ request, params }) => {
			sent.push({ request: params.request, body: await request.json() });
			return new HttpResponse(null, { status: 202 });
		}),
	);
	return sent;
}

function card(request: AgentRequest, outcome?: RequestOutcome) {
	return renderWithQuery(
		<RequestCard
			chatId="s1"
			request={request}
			pending={!outcome}
			outcome={outcome}
			agent="Claude Code"
		/>,
	);
}

function tool(
	name: string,
	input: Record<string, unknown>,
	extra: { canAllowForSession?: boolean; native?: Record<string, unknown> } = {},
): AgentRequest {
	return {
		id: "r1",
		native: extra.native ?? {},
		kind: { type: "tool", name, input, canAllowForSession: extra.canAllowForSession ?? false },
	};
}

describe("a permission", () => {
	it("shows the command and its folder, and allows it for the session", async () => {
		const sent = answers();
		card(
			tool(
				"Bash",
				{ command: "cargo test", cwd: "/work/app" },
				{
					canAllowForSession: true,
					native: {
						permission_suggestions: [
							{ type: "addRules", rules: [{ toolName: "Bash", ruleContent: "cargo test:*" }] },
						],
					},
				},
			),
		);
		expect(screen.getByText("Bash cargo test")).toBeInTheDocument();
		expect(screen.getByText("cargo test")).toBeInTheDocument();
		expect(screen.getByText("in /work/app")).toBeInTheDocument();
		expect(screen.getByText(/also allows Bash\(cargo test:\*\)/)).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Allow for this session" }));
		await expect
			.poll(() => sent)
			.toEqual([{ request: "r1", body: { decision: "allowForSession" } }]);
	});

	it("shows an edit as a diff and denies it with a reason", async () => {
		const sent = answers();
		card(tool("Edit", { file_path: "a.rs", old_string: "old", new_string: "new" }));
		expect(
			screen.queryByRole("button", { name: "Allow for this session" }),
		).not.toBeInTheDocument();
		expect(screen.getByText("-old")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Add a reason" }));
		await userEvent.type(screen.getByLabelText(/Why not/), "Wrong file{Enter}");
		await expect
			.poll(() => sent)
			.toEqual([{ request: "r1", body: { decision: "deny", message: "Wrong file" } }]);
	});

	it("shows other input as fields, and Deny and stop cancels", async () => {
		const sent = answers();
		card(tool("mcp__github__issue", { repo: "ahab", title: "Bug" }));
		expect(screen.getByText("repo")).toBeInTheDocument();
		expect(screen.getByText("Bug")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Deny and stop" }));
		await expect.poll(() => sent).toEqual([{ request: "r1", body: { decision: "cancel" } }]);
	});

	it("answers with the focused card's keys", async () => {
		const sent = answers();
		card(tool("Bash", { command: "ls" }));
		const region = screen.getByRole("region", { name: "Bash ls" });
		region.focus();
		fireEvent.keyDown(region, { key: "Escape", code: "Escape" });
		await expect.poll(() => sent).toEqual([{ request: "r1", body: { decision: "deny" } }]);
		fireEvent.keyDown(region, { key: "Enter", code: "Enter" });
		await expect.poll(() => sent).toHaveLength(2);
		expect(sent[1]).toEqual({ request: "r1", body: { decision: "allow" } });
	});

	it("offers an ACP agent's own choices", async () => {
		const sent = answers();
		card({
			id: "r1",
			native: {},
			kind: {
				type: "permission",
				name: "edit",
				displayName: "Edit a.rs",
				input: {},
				options: [
					{ id: "once", label: "Allow once", kind: "allowOnce" },
					{ id: "always", label: "Always allow", kind: "allowAlways" },
					{ id: "no", label: "Reject", kind: "rejectOnce" },
				],
			},
		});
		await userEvent.click(screen.getByRole("button", { name: "Always allow" }));
		await expect
			.poll(() => sent)
			.toEqual([{ request: "r1", body: { decision: "select", optionId: "always" } }]);
	});

	it("says how it ended once answered", () => {
		card(tool("Bash", { command: "ls" }), {
			type: "answered",
			decision: { type: "deny", message: "No" },
		});
		expect(screen.getByText("Denied: No")).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
	});
});

const questions: AgentRequest = {
	id: "q1",
	native: {},
	kind: {
		type: "question",
		toolCallId: "toolu_1",
		questions: [
			{
				id: "Which library?",
				header: "Library",
				question: "Which library?",
				options: [
					{ label: "chrono", description: "Full-featured" },
					{ label: "time", description: "Small and fast", preview: "`time = 0.3`" },
				],
				multiSelect: false,
				allowOther: true,
				secret: false,
			},
			{
				id: "Which features?",
				header: "Features",
				question: "Which features?",
				options: [{ label: "serde" }, { label: "macros" }],
				multiSelect: true,
				allowOther: false,
				secret: false,
			},
		],
	},
};

describe("a question", () => {
	it("steps through its questions and sends every answer", async () => {
		const sent = answers();
		card(questions);
		expect(screen.getByText("Question 1 of 2")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
		await userEvent.click(screen.getByRole("radio", { name: /time/ }));
		expect(screen.getByText("time = 0.3")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("radio", { name: "Other…" }));
		expect(screen.getByRole("radio", { name: /time/ })).not.toBeChecked();
		await userEvent.type(screen.getByLabelText("Your own answer"), "jiff");
		await userEvent.click(screen.getByRole("button", { name: "Next" }));
		expect(screen.getByText("Question 2 of 2")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("checkbox", { name: "serde" }));
		await userEvent.click(screen.getByRole("checkbox", { name: "macros" }));
		await userEvent.click(screen.getByRole("button", { name: "Submit" }));
		await expect
			.poll(() => sent)
			.toEqual([
				{
					request: "q1",
					body: {
						decision: "answer",
						answers: [
							{ questionId: "Which library?", selected: [], other: "jiff" },
							{ questionId: "Which features?", selected: ["serde", "macros"] },
						],
					},
				},
			]);
	});

	it("uses arrow keys for a single choice and requires text for a selected custom answer", async () => {
		const sent = answers();
		card(questions);
		await userEvent.click(screen.getByRole("radio", { name: "chrono" }));
		await userEvent.keyboard("{ArrowDown}");
		expect(screen.getByRole("radio", { name: "time" })).toBeChecked();
		await userEvent.keyboard("{ArrowDown}");
		expect(screen.getByRole("radio", { name: "Other…" })).toBeChecked();
		expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
		expect(sent).toHaveLength(0);
	});

	it("hides a secret answer", () => {
		card({
			id: "q2",
			native: {},
			kind: {
				type: "question",
				questions: [
					{
						id: "token",
						question: "Your token?",
						options: [],
						multiSelect: false,
						allowOther: false,
						secret: true,
					},
				],
			},
		});
		expect(screen.getByLabelText("Your answer, kept hidden")).toHaveAttribute("type", "password");
	});

	it("stays, showing what was chosen", () => {
		card(questions, {
			type: "answered",
			decision: {
				type: "answer",
				answers: [
					{ questionId: "Which library?", selected: ["time"] },
					{ questionId: "Which features?", selected: ["serde", "macros"] },
				],
			},
		});
		expect(screen.getByText("Claude Code asks")).toBeInTheDocument();
		expect(screen.getByText("time")).toBeInTheDocument();
		expect(screen.getByText("serde and macros")).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Submit" })).not.toBeInTheDocument();
	});
});

describe("a plan", () => {
	const plan: AgentRequest = {
		id: "p1",
		native: {},
		kind: {
			type: "planApproval",
			plan: "# Add dark mode\n\n1. Tokens\n2. Toggle",
			path: "/home/me/.claude/plans/dark.md",
			options: [
				{ id: "acceptEdits", label: "Yes, and accept edits without asking", kind: "allowOnce" },
				{ id: "default", label: "Yes, and ask before each edit", kind: "allowOnce" },
			],
		},
	};

	it("renders the plan and approves it with an option", async () => {
		const sent = answers();
		card(plan);
		expect(screen.getByRole("heading", { name: "Add dark mode" })).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Yes, and ask before each edit" }));
		await expect
			.poll(() => sent)
			.toEqual([{ request: "p1", body: { decision: "select", optionId: "default" } }]);
	});

	it("keeps planning with feedback", async () => {
		const sent = answers();
		card(plan);
		await userEvent.click(screen.getByRole("button", { name: "Keep planning" }));
		await userEvent.type(screen.getByLabelText("What should change?"), "Smaller steps");
		await userEvent.click(screen.getByRole("button", { name: "Send back" }));
		await expect
			.poll(() => sent)
			.toEqual([{ request: "p1", body: { decision: "deny", message: "Smaller steps" } }]);
	});
});

describe("a form", () => {
	it("checks its fields and submits their values", async () => {
		const sent = answers();
		card({
			id: "f1",
			native: {},
			kind: {
				type: "form",
				message: "Which repository?",
				source: "github",
				schema: {},
				fields: [
					{ name: "repo", title: "Repository", kind: "text", required: true, options: [] },
					{
						name: "count",
						title: "How many",
						kind: "integer",
						required: false,
						options: [],
						minimum: 1,
						maximum: 10,
					},
					{
						name: "kind",
						title: "Kind",
						kind: "select",
						required: false,
						options: [
							{ value: "bug", label: "Bug" },
							{ value: "idea", label: "Idea" },
						],
					},
					{ name: "draft", title: "Draft", kind: "boolean", required: false, options: [] },
				],
			},
		});
		expect(screen.getByText("github asks for details")).toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Submit" }));
		expect(screen.getByText("Required.")).toBeInTheDocument();
		expect(sent).toEqual([]);
		await userEvent.type(screen.getByLabelText(/Repository/), "ahab");
		await userEvent.type(screen.getByLabelText("How many"), "20");
		await userEvent.click(screen.getByRole("button", { name: "Submit" }));
		expect(screen.getByText("At most 10.")).toBeInTheDocument();
		await userEvent.clear(screen.getByLabelText("How many"));
		await userEvent.type(screen.getByLabelText("How many"), "3");
		await userEvent.selectOptions(screen.getByLabelText("Kind"), "idea");
		await userEvent.click(screen.getByRole("switch", { name: "Draft" }));
		await userEvent.click(screen.getByRole("button", { name: "Submit" }));
		await expect
			.poll(() => sent)
			.toEqual([
				{
					request: "f1",
					body: {
						decision: "submit",
						values: { repo: "ahab", count: 3, kind: "idea", draft: true },
					},
				},
			]);
	});

	it("opens a page and declines", async () => {
		const sent = answers();
		card({
			id: "f2",
			native: {},
			kind: {
				type: "form",
				message: "Sign in to GitHub",
				url: "https://github.com/login/device",
				schema: {},
				fields: [],
			},
		});
		expect(screen.getByRole("link", { name: "Open the page" })).toHaveAttribute(
			"href",
			"https://github.com/login/device",
		);
		await userEvent.click(screen.getByRole("button", { name: "Decline" }));
		await expect.poll(() => sent).toEqual([{ request: "f2", body: { decision: "deny" } }]);
	});
});
