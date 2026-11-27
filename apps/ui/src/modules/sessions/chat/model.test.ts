import { describe, expect, it } from "vitest";

import {
	relativeTo,
	agentConversation,
	apply,
	childAgents,
	type Conversation,
	empty,
	isVisibleEntry,
	itemText,
	effortLabel,
	requestSummary,
	requestPreview,
	sessionScope,
	ruleKeptIn,
	answerFrom,
	answered,
	toggleOption,
	toggleOther,
	emptyDraft,
	fieldProblem,
	formDefaults,
	formValues,
	outcomeSummary,
	shownAnswers,
	type AgentRequest,
	type AgentQuestion,
	type AgentFormField,
	active,
	observed,
	historyUpTo,
	transcript,
	turnAnswer,
	railTurns,
	turnStarts,
	workedFor,
	tokens,
	hasArrived,
} from "@/modules/sessions/chat/model";

function run(events: [number | null, Record<string, unknown>][]): Conversation {
	return events.reduce<Conversation>(
		(conversation, [seq, event]) => apply(conversation, { seq, event: event as { type: string } }),
		empty,
	);
}

const usage = (total: number) => ({
	type: "usage",
	turnId: "t",
	usage: { last: { input: total, output: 0, total }, contextWindow: 200_000 },
});

describe("compaction", () => {
	it("pairs a start and end without an id, and takes the size after", () => {
		const running = run([
			[1, usage(150_000)],
			[2, { type: "compaction", turnId: "t", compaction: { id: null, status: "inProgress" } }],
		]);
		expect(running.compacting).toBe("compaction:2");
		expect(running.entries.at(-1)).toMatchObject({ kind: "compaction", status: "inProgress" });

		const done = apply(running, {
			seq: 3,
			event: {
				type: "compaction",
				turnId: "t",
				compaction: {
					id: null,
					status: "completed",
					trigger: "auto",
					tokensBefore: 150_000,
					tokensAfter: 9_000,
				},
			},
		});
		expect(done.compacting).toBeUndefined();
		expect(done.entries.filter((entry) => entry.kind === "compaction")).toEqual([
			{
				key: "compaction:2",
				kind: "compaction",
				status: "completed",
				trigger: "auto",
				tokensBefore: 150_000,
				tokensAfter: 9_000,
			},
		]);
		expect(done.context).toEqual({ used: 9_000, window: 200_000 });
	});

	it("marks the size stale when the agent does not report it, until the next call", () => {
		const compacted = run([
			[1, usage(150_000)],
			[2, { type: "compaction", turnId: "t", compaction: { id: "c1", status: "inProgress" } }],
			[3, { type: "compaction", turnId: "t", compaction: { id: "c1", status: "completed" } }],
		]);
		expect(compacted.entries.filter((entry) => entry.kind === "compaction")).toHaveLength(1);
		expect(compacted.context).toEqual({ used: 150_000, window: 200_000, stale: true });
		expect(apply(compacted, { seq: 4, event: usage(12_000) }).context).toEqual({
			used: 12_000,
			window: 200_000,
		});
	});

	it("keeps the size when a compaction fails, and ends one the chat stopped during", () => {
		const failed = run([
			[1, usage(150_000)],
			[2, { type: "compaction", compaction: { status: "inProgress" } }],
			[3, { type: "compaction", compaction: { status: "failed", error: "too long" } }],
		]);
		expect(failed.entries.at(-1)).toMatchObject({ status: "failed", error: "too long" });
		expect(failed.context).toEqual({ used: 150_000, window: 200_000 });

		const stopped = run([
			[1, { type: "compaction", compaction: { status: "inProgress" } }],
			[2, { type: "closed", reason: { type: "client" } }],
		]);
		expect(stopped.compacting).toBeUndefined();
		expect(stopped.entries.at(-1)).toMatchObject({ kind: "compaction", status: "cancelled" });
	});
});

describe("conversation", () => {
	it("streams text into an item and keeps the final text", () => {
		const live = run([
			[1, { type: "turnStart", turnId: "t" }],
			[2, { type: "itemStart", turnId: "t", item: { type: "agentMessage", id: "a", text: "" } }],
			[null, { type: "itemDelta", turnId: "t", itemId: "a", delta: { type: "text", text: "Hel" } }],
			[null, { type: "itemDelta", turnId: "t", itemId: "a", delta: { type: "text", text: "lo" } }],
		]);
		expect(live.working).toBe(true);
		const [entry] = live.entries;
		expect(entry?.kind === "item" && itemText(entry)).toBe("Hello");

		const done = apply(live, {
			seq: 3,
			event: {
				type: "itemEnd",
				turnId: "t",
				item: { type: "agentMessage", id: "a", text: "Hello!" },
			},
		});
		const [final] = done.entries;
		expect(final?.kind === "item" && itemText(final)).toBe("Hello!");
	});

	it("skips empty Thinking summaries and keeps the footer on the last visible item", () => {
		const item = { type: "thinking", id: "a", text: "" };
		const conversation = run([
			[1, { type: "itemStart", item }],
			[null, { type: "itemDelta", itemId: "a", delta: { text: "Checking the result" } }],
			[2, { type: "itemEnd", item }],
			[3, { type: "itemEnd", item: { ...item, id: "b", text: "  " } }],
			[4, { type: "turnEnd", turn: { id: "t", status: "completed" } }],
		]);
		const visible = conversation.entries.filter(isVisibleEntry);
		expect(visible).toHaveLength(1);
		expect(visible[0]?.kind === "item" && itemText(visible[0])).toBe("Checking the result");
		expect(conversation.finished?.[0]?.after).toBe("item:a");
	});

	it("hides items that show nothing, so they leave no gap", () => {
		const conversation = run([
			[1, { type: "itemEnd", item: { type: "other", id: "o", kind: "webSearch", raw: {} } }],
			[2, { type: "itemEnd", item: { type: "agentMessage", id: "e", text: " " } }],
			[3, { type: "itemEnd", item: { type: "plan", id: "p", text: "" } }],
			[4, { type: "itemStart", item: { type: "agentMessage", id: "s", text: "" } }],
		]);
		const visible = conversation.entries.filter(isVisibleEntry).map((entry) => entry.key);
		// Only the answer still streaming (its cursor shows).
		expect(visible).toEqual(["item:s"]);
	});

	it("keeps when a turn ended", () => {
		const conversation = [
			{ seq: 1, event: { type: "turnStart", turnId: "t" }, at: "2026-11-23T08:00:00Z" },
			{
				seq: 2,
				event: { type: "itemEnd", item: { type: "agentMessage", id: "a", text: "Done" } },
			},
			{
				seq: 3,
				event: { type: "turnEnd", turn: { id: "t", status: "completed" } },
				at: "2026-11-23T08:00:39Z",
			},
		].reduce<Conversation>(apply, empty);
		expect(conversation.finished?.[0]).toMatchObject({
			durationMs: 39_000,
			endedAt: Date.parse("2026-11-23T08:00:39Z"),
		});
	});

	it("shows the messages waiting for the turn to end, as last told", () => {
		const queued = run([
			[
				null,
				{
					type: "queue",
					items: [{ id: "q", text: "and then", attachments: 0, interrupt: false }],
				},
			],
		]);
		expect(queued.queue).toEqual([{ id: "q", text: "and then", attachments: 0, interrupt: false }]);
		expect(apply(queued, { seq: null, event: { type: "queue", items: [] } }).queue).toEqual([]);
	});

	it("ignores events it has already seen", () => {
		const once = run([[1, { type: "error", message: "x", retrying: false }]]);
		expect(apply(once, { seq: 1, event: { type: "error", message: "x", retrying: false } })).toBe(
			once,
		);
	});

	it("notes errors and failed turns, not stops or the end", () => {
		const conversation = run([
			[1, { type: "turnStart", turnId: "t" }],
			[2, { type: "error", message: "rate limited", retrying: true }],
			[3, { type: "error", message: "boom", retrying: false }],
			[4, { type: "turnEnd", turn: { id: "t", status: "interrupted" } }],
			[5, { type: "turnStart", turnId: "u" }],
			[6, { type: "turnEnd", turn: { id: "u", status: "failed", error: "overloaded" } }],
			[7, { type: "modelChanged", model: "opus" }],
			[8, { type: "closed", reason: { type: "exited", message: "killed" } }],
		]);
		expect(conversation.working).toBe(false);
		expect(conversation.entries.map((entry) => entry.kind)).toEqual(["error", "turnEnd"]);
	});

	it("notes that a resumed agent did not take the chat's saved mode", () => {
		const conversation = run([
			[1, { type: "modeUnavailable", mode: "retired", current: "ask" }],
			[2, { type: "modeUnavailable", mode: "gone", current: null }],
		]);
		expect(conversation.entries).toEqual([
			{
				key: "modeUnavailable:1",
				kind: "notice",
				message: "The agent no longer offers the mode retired, so it continued in ask.",
			},
			{
				key: "modeUnavailable:2",
				kind: "notice",
				message: "The agent no longer offers the mode gone, so it continued in its current mode.",
			},
		]);
	});

	it.each(["client", "agent"])("silently closes a chat after a routine %s stop", (type) => {
		const chat = run([
			[1, { type: "turnStart", turnId: "t" }],
			[2, { type: "closed", reason: { type } }],
		]);
		expect(chat.working).toBe(false);
		expect(chat.entries).toEqual([]);
	});

	it("notes where each finished turn ends", () => {
		const conversation = run([
			[1, { type: "turnStart", turnId: "t1" }],
			[2, { type: "itemEnd", turnId: "t1", item: { type: "agentMessage", id: "a", text: "Done" } }],
			[3, { type: "turnEnd", turn: { id: "t1", status: "completed" } }],
			[4, { type: "turnStart", turnId: "t2" }],
		]);
		expect(conversation.finished).toEqual([
			{ turn: "t1", after: "item:a", seq: 3, answer: "Done" },
		]);
		expect(conversation.turn).toBe("t2");
	});

	it("tracks how full the context is", () => {
		const conversation = run([
			[
				1,
				{
					type: "usage",
					turnId: "t",
					usage: { last: { input: 30_000, output: 2_000, total: 32_000 }, contextWindow: 200_000 },
				},
			],
			[
				2,
				{
					type: "usage",
					turnId: "t",
					usage: { last: { input: 40_000, output: 0, total: 0 }, contextWindow: null },
				},
			],
		]);
		expect(conversation.context).toEqual({ used: 40_000, window: 200_000 });
		expect(tokens(950)).toBe("950");
		expect(tokens(38_400)).toBe("38k");
		expect(tokens(1_000_000)).toBe("1M");
	});

	it("says what a request asks", () => {
		expect(requestSummary(tool("Write", { file_path: "a.rs" }))).toBe("Write a.rs");
		expect(requestSummary(tool("Bash", { command: "ls" }))).toBe("Bash ls");
		expect(requestSummary(tool("commandExecution", { command: ["cargo", "test"] }))).toBe(
			"commandExecution cargo test",
		);
		const described: AgentRequest = {
			id: "r",
			native: {},
			kind: {
				type: "tool",
				name: "X",
				description: "does y",
				input: {},
				canAllowForSession: false,
			},
		};
		expect(requestSummary(described)).toBe("X: does y");
		expect(requestSummary(question([libraryQuestion]))).toBe("Which library?");
		expect(requestSummary(plan("# Plan"))).toBe("Approve the plan");
		expect(effortLabel("xhigh")).toBe("Extra high");
		expect(effortLabel("high")).toBe("High");
	});
});

/** A tool permission, as Stubb sends it. */
function tool(name: string, input: Record<string, unknown>, native: unknown = {}): AgentRequest {
	return {
		id: "r",
		native: native as AgentRequest["native"],
		kind: { type: "tool", name, input, canAllowForSession: false },
	};
}

const libraryQuestion: AgentQuestion = {
	id: "Which library?",
	header: "Library",
	question: "Which library?",
	options: [
		{ label: "chrono", description: "Full-featured" },
		{ label: "time", description: "Small" },
	],
	multiSelect: false,
	allowOther: true,
	secret: false,
};

function question(questions: AgentQuestion[]): AgentRequest {
	return { id: "q", native: {}, kind: { type: "question", questions } };
}

function plan(text: string): AgentRequest {
	return {
		id: "p",
		native: {},
		kind: {
			type: "planApproval",
			plan: text,
			options: [{ id: "acceptEdits", label: "Yes, and accept edits", kind: "allowOnce" }],
		},
	};
}

describe("requests in the conversation", () => {
	it("keeps each request where it was asked, with how it ended", () => {
		const conversation = run([
			[1, { type: "turnStart", turnId: "t1" }],
			[2, { type: "request", request: question([libraryQuestion]) }],
			[3, { type: "requestDeclined", method: "mcpServer/elicitation", summary: "github" }],
			[
				4,
				{
					type: "requestResolved",
					id: "q",
					outcome: {
						type: "answered",
						decision: {
							type: "answer",
							answers: [{ questionId: "Which library?", selected: ["time"] }],
						},
					},
				},
			],
			[5, { type: "request", request: plan("# Plan") }],
			[6, { type: "requestResolved", id: "p", outcome: { type: "withdrawn" } }],
		]);
		const [asked, declined, planned] = conversation.entries;
		expect(asked?.kind === "request" && asked.outcome?.type).toBe("answered");
		expect(declined).toMatchObject({ kind: "declined", summary: "github" });
		expect(planned?.kind === "request" && planned.outcome).toEqual({ type: "withdrawn" });
		if (asked?.kind !== "request") throw new Error("a request");
		expect(shownAnswers(asked.request, asked.outcome)).toEqual([
			{ question: libraryQuestion, answer: "time" },
		]);
		if (planned?.kind !== "request") throw new Error("a request");
		expect(outcomeSummary(planned.request, planned.outcome)).toBe("Withdrawn by the agent");
		expect(
			outcomeSummary(planned.request, {
				type: "answered",
				decision: { type: "select", optionId: "acceptEdits" },
			}),
		).toBe("Yes, and accept edits");
		expect(
			outcomeSummary(planned.request, {
				type: "answered",
				decision: { type: "deny", message: "Smaller" },
			}),
		).toBe("Sent back: Smaller");
		expect(
			outcomeSummary(tool("Bash", {}), { type: "answered", decision: { type: "cancel" } }),
		).toBe("Denied, and the turn stopped");
	});

	it("previews a permission by its tool", () => {
		expect(requestPreview(tool("Bash", { command: "cargo test", description: "x" }))).toEqual({
			type: "command",
			command: "cargo test",
			cwd: undefined,
		});
		expect(
			requestPreview(tool("commandExecution", { command: "ls", cwd: "/work/app/src" })),
		).toMatchObject({ type: "command", command: "ls", cwd: "/work/app/src" });
		expect(
			requestPreview(tool("Edit", { file_path: "a.rs", old_string: "a\nb", new_string: "c" })),
		).toEqual({ type: "diff", files: [{ path: "a.rs", diff: "-a\n-b\n+c" }] });
		expect(requestPreview(tool("Write", { file_path: "b.rs", content: "x\n" }))).toEqual({
			type: "diff",
			files: [{ path: "b.rs", diff: "+x" }],
		});
		expect(requestPreview(tool("WebFetch", { url: "https://a.b", prompt: "title?" }))).toEqual({
			type: "url",
			url: "https://a.b",
			prompt: "title?",
		});
		expect(requestPreview(tool("mcp__x__y", { repo: "ahab", depth: 2 }))).toEqual({
			type: "fields",
			fields: [
				{ key: "repo", value: "ahab", long: false },
				{ key: "depth", value: "2", long: false },
			],
		});
		// Codex's file change carries its diff in the tool call.
		expect(
			requestPreview(tool("fileChange", {}), {
				type: "toolCall",
				id: "c",
				status: "inProgress",
				detail: {
					kind: "fileChange",
					changes: [{ path: "a.rs", kind: { type: "update" }, diff: "@@\n-a\n+b" }],
				},
			}),
		).toEqual({ type: "diff", files: [{ path: "a.rs", diff: "@@\n-a\n+b" }] });
		expect(requestPreview(tool("Other", {}))).toEqual({ type: "none" });
	});

	it("says what allowing for the session covers", () => {
		const suggested = tool(
			"Bash",
			{ command: "npm test" },
			{
				permission_suggestions: [
					{ type: "addRules", rules: [{ toolName: "Bash", ruleContent: "npm test:*" }] },
					{ type: "setMode", mode: "acceptEdits" },
				],
			},
		);
		expect(sessionScope(suggested)).toBe("Bash(npm test:*) and every file edit");
		expect(ruleKeptIn(suggested)).toBeUndefined();
		const kept = tool(
			"Bash",
			{},
			{
				permission_suggestions: [
					{ type: "addRules", rules: [{ toolName: "Bash" }], destination: "localSettings" },
				],
			},
		);
		expect(ruleKeptIn(kept)).toBe("localSettings");
		expect(sessionScope(tool("commandExecution", {}))).toBe("this command");
		expect(sessionScope(tool("Read", {}))).toBeUndefined();
	});

	it("answers questions from what was picked and written", () => {
		const multi: AgentQuestion = {
			...libraryQuestion,
			id: "features",
			question: "Which features?",
			multiSelect: true,
		};
		const secret: AgentQuestion = {
			...libraryQuestion,
			id: "token",
			question: "Token?",
			options: [],
			allowOther: false,
			secret: true,
		};
		let draft = toggleOption(libraryQuestion, emptyDraft, "chrono");
		draft = toggleOption(libraryQuestion, draft, "time");
		expect(draft.selected).toEqual(["time"]);
		draft = toggleOther(libraryQuestion, draft);
		expect(draft.selected).toEqual([]);
		expect(answered(libraryQuestion, draft)).toBe(false);
		draft = { ...draft, other: " jiff " };
		expect(answered(libraryQuestion, draft)).toBe(true);
		let many = toggleOption(multi, emptyDraft, "chrono");
		many = toggleOption(multi, many, "time");
		expect(answered(multi, toggleOther(multi, many))).toBe(false);
		many = toggleOther(multi, { ...many, other: "serde" });
		expect(many.selected).toEqual(["chrono", "time"]);
		const answer = answerFrom([libraryQuestion, multi, secret], {
			"Which library?": draft,
			features: many,
			token: { ...emptyDraft, other: "s3cret" },
		});
		expect(answer).toEqual({
			decision: "answer",
			answers: [
				{ questionId: "Which library?", selected: [], other: "jiff" },
				{ questionId: "features", selected: ["chrono", "time"], other: "serde" },
				{ questionId: "token", selected: [], other: "s3cret" },
			],
		});
		const request = question([libraryQuestion, secret]);
		expect(
			shownAnswers(request, {
				type: "answered",
				decision: {
					type: "answer",
					answers: [
						{ questionId: "Which library?", selected: ["time"], other: "pinned" },
						{ questionId: "token", selected: [], other: "s3cret" },
					],
				},
			}).map((each) => each.answer),
		).toEqual(["time and pinned", "••••••"]);
	});

	it("checks and sends a form's values", () => {
		const fields: AgentFormField[] = [
			{ name: "email", kind: "text", required: true, options: [], format: "email" },
			{ name: "age", kind: "integer", required: false, options: [], minimum: 0, maximum: 150 },
			{ name: "ok", kind: "boolean", required: false, options: [], default: true },
			{ name: "tags", kind: "multiSelect", required: false, options: [], maximum: 1 },
		];
		const field = (name: string) => fields.find((each) => each.name === name) as AgentFormField;
		expect(formDefaults(fields)).toEqual({ ok: true, tags: [] });
		expect(fieldProblem(field("email"), "")).toBe("Required.");
		expect(fieldProblem(field("age"), "")).toBeUndefined();
		expect(fieldProblem(field("age"), "2.5")).toBe("Enter a whole number.");
		expect(fieldProblem(field("age"), "200")).toBe("At most 150.");
		expect(fieldProblem(field("tags"), ["a", "b"])).toBe("Pick at most 1.");
		expect(formValues(fields, { email: "a@b.c", age: "42", ok: false, tags: [] })).toEqual({
			email: "a@b.c",
			age: 42,
			ok: false,
			tags: [],
		});
	});
});

describe("paths in a chat", () => {
	it("shows paths inside the project relative to it", () => {
		expect(relativeTo("/work/app", "/work/app/src/main.rs")).toBe("src/main.rs");
		expect(relativeTo("/work/app/", "/work/app/a.ts")).toBe("a.ts");
		expect(relativeTo("/work/app", "/work/apple/a.ts")).toBe("/work/apple/a.ts");
		expect(relativeTo(undefined, "/work/app/a.ts")).toBe("/work/app/a.ts");
		const request = tool("Write", { file_path: "/work/app/a.rs" });
		expect(requestSummary(request, (path) => relativeTo("/work/app", path))).toBe("Write a.rs");
	});
});

const user = (id: string, text: string) => ({
	seq: null,
	event: {
		type: "itemEnd",
		item: { type: "userMessage", id, content: [{ type: "text", text }] },
	},
});
const said = (id: string, text: string) => ({
	seq: null,
	event: { type: "itemEnd", item: { type: "agentMessage", id, text } },
});

describe("turns", () => {
	it("time a turn by the agent's report, else by its events", () => {
		let conversation = [
			{ seq: 1, event: { type: "turnStart", turnId: "t1" }, at: "2026-11-19T10:00:00Z" },
			{ ...said("a", "Done."), seq: 2 },
			{ seq: 3, event: { type: "turnEnd", turn: { id: "t1" } }, at: "2026-11-19T10:02:05Z" },
		].reduce(apply, empty);
		expect(conversation.finished?.[0]?.durationMs).toBe(125_000);
		expect(workedFor(125_000)).toBe("2m 5s");
		conversation = [
			{ seq: 4, event: { type: "turnStart", turnId: "t2" } },
			{ seq: 5, event: { type: "turnEnd", turn: { id: "t2", durationMs: 4_000 } } },
		].reduce(apply, conversation);
		expect(conversation.finished?.[1]?.durationMs).toBe(4_000);
		expect(workedFor(3_725_000)).toBe("1h 2m");
	});

	it("copy the conversation, a turn's answer, and find where turns start", () => {
		const { entries } = [
			user("u1", "Fix it"),
			said("a1", "Looking."),
			said("a2", "Fixed."),
			user("u2", "Thanks"),
			said("a3", "Welcome."),
		].reduce(apply, empty);
		expect(transcript(entries, "Claude Code", "You")).toBe(
			"## You\n\nFix it\n\n## Claude Code\n\nLooking.\n\n## Claude Code\n\nFixed.\n\n## You\n\nThanks\n\n## Claude Code\n\nWelcome.\n",
		);
		expect(turnAnswer(entries, "item:a2")).toBe("Looking.\n\nFixed.");
		expect(turnStarts(entries).map((start) => start.text)).toEqual(["Fix it", "Thanks"]);
	});

	it("carry the history up to a turn to a fork, the earliest left out when too long", () => {
		const { entries } = [
			user("u1", "Fix it"),
			said("a1", "Fixed."),
			user("u2", "Thanks"),
			said("a2", "Welcome."),
		].reduce(apply, empty);
		expect(historyUpTo(entries, "item:a1", "Codex", "You", "(cut)")).toBe(
			"## You\n\nFix it\n\n## Codex\n\nFixed.\n",
		);
		const long = [user("u1", "x".repeat(200 * 1024)), said("a1", "y".repeat(100 * 1024))].reduce(
			apply,
			empty,
		);
		const cut = historyUpTo(long.entries, "item:a1", "Codex", "You", "(cut)");
		expect(cut.startsWith("(cut)\n\n## Codex")).toBe(true);
	});
});

describe("subagents and background tasks", () => {
	it("keep the latest picture, without the chat's own agent or housekeeping, until it stops", () => {
		let conversation = [
			{
				seq: null,
				event: {
					type: "observation",
					rootAgentId: "root",
					agents: [
						{ id: "root", parent: { type: "root" }, status: "running" },
						{
							id: "sub",
							parent: { type: "agent", id: "root" },
							name: "explorer",
							status: "running",
						},
					],
					tasks: [
						{
							id: "dev",
							kind: { type: "command", command: "pnpm dev" },
							status: "running",
							background: true,
						},
						{ id: "run", kind: { type: "agentRun", agentId: "sub" }, status: "running" },
						{
							id: "gc",
							kind: { type: "other", name: "compact" },
							status: "running",
							ambient: true,
						},
					],
				},
			},
		].reduce(apply, empty);
		const shown = observed(conversation.observation);
		expect(shown.agents.map((agent) => agent.id)).toEqual(["sub"]);
		expect(shown.tasks.map((task) => task.id)).toEqual(["dev"]);
		expect(active("running")).toBe(true);
		expect(active("completed")).toBe(false);
		conversation = apply(conversation, {
			seq: 9,
			event: { type: "closed", reason: { type: "agent" } },
		});
		expect(conversation.observation).toBeUndefined();
	});
});

describe("subagents", () => {
	const observation = {
		rootAgentId: "root",
		agents: [
			{ id: "root", parent: { type: "root" as const }, status: "running" },
			{ id: "a", parent: { type: "agent" as const, id: "root" }, status: "running" },
			{ id: "b", parent: { type: "agent" as const, id: "a" }, status: "completed" },
			{ id: "c", parent: { type: "unknown" as const }, status: "running" },
		],
		tasks: [],
	};

	it("lists each agent's own subagents, unknown parents under the chat's", () => {
		expect(childAgents(observation, undefined).map((agent) => agent.id)).toEqual(["a", "c"]);
		expect(childAgents(observation, "a").map((agent) => agent.id)).toEqual(["b"]);
		expect(childAgents(observation, "b")).toEqual([]);
	});

	it("keeps each subagent's live events apart from the chat's", () => {
		const conversation = run([
			[null, { type: "agentActivity", agentId: "a", event: { type: "turnStart", turnId: "t" } }],
			[
				null,
				{
					type: "agentActivity",
					agentId: "a",
					event: { type: "itemDelta", itemId: "m", delta: { type: "text", text: "Hi" } },
				},
			],
		]);
		expect(conversation.entries).toEqual([]);
		expect(conversation.agents?.a).toHaveLength(2);
		const shown = agentConversation(undefined, conversation.agents?.a);
		expect(shown.working).toBe(true);
		expect(shown.entries.map((entry) => entry.key)).toEqual(["item:m"]);
	});

	it("shows its stored conversation, then what it did live, once each", () => {
		const message = { type: "agentMessage", id: "m", text: "Done." };
		const turn = { id: "t", status: "completed", items: [message] };
		const history = [
			{ type: "turnStart", turnId: "t" },
			{ type: "itemEnd", turnId: "t", item: message },
			{ type: "turnEnd", turn },
		];
		const shown = agentConversation(history, [
			{ seq: null, event: { type: "itemEnd", turnId: "t", item: message } },
			{ seq: null, event: { type: "turnEnd", turn } },
		]);
		expect(shown.entries).toHaveLength(1);
		expect(shown.finished).toHaveLength(1);
		expect(shown.working).toBe(false);
	});
});

it("shows stored Codex delegated tool items and retains their native IDs for navigation", () => {
	const next = apply(empty, {
		seq: 1,
		event: {
			type: "itemEnd",
			item: {
				type: "other",
				id: "delegate-1",
				kind: "collabAgentToolCall",
				raw: { tool: "wait", status: "completed" },
			},
		},
	});
	expect(next.entries[0]).toMatchObject({
		kind: "item",
		item: {
			type: "toolCall",
			id: "delegate-1",
			detail: { kind: "other", name: "Agent", input: { tool: "wait" } },
		},
	});
});

const echo = (id: string, submissionId?: string) => ({
	type: "itemEnd",
	turnId: "t",
	item: { type: "userMessage", id, content: [{ type: "text", text: "hi" }], submissionId },
});

it("matches a sent message to its echo by submission ID, not by any new user message", () => {
	const other = run([[1, echo("u1")]]);
	expect(hasArrived(other.entries, other.queue, "mine")).toBe(false);
	const someoneElse = run([[1, echo("u1", "theirs")]]);
	expect(hasArrived(someoneElse.entries, someoneElse.queue, "mine")).toBe(false);
	const echoed = run([[1, echo("u1", "mine")]]);
	expect(hasArrived(echoed.entries, echoed.queue, "mine")).toBe(true);
	expect(
		hasArrived([], [{ id: "mine", text: "hi", attachments: 0, interrupt: false }], "mine"),
	).toBe(true);
});

it("puts every turn on the rail, loaded or not, with a message just sent", () => {
	const all = [
		{ seq: 3, text: "First" },
		{ seq: 40, text: "Second" },
	];
	expect(
		railTurns(all, [{ seq: 40, text: "Second" }, { seq: 52, text: "Third" }, { text: "Sending" }]),
	).toEqual([
		{ seq: 3, text: "First" },
		{ seq: 40, text: "Second" },
		{ seq: 52, text: "Third" },
	]);
	expect(railTurns(undefined, [{ seq: 7, text: "Only" }])).toEqual([{ seq: 7, text: "Only" }]);
});
