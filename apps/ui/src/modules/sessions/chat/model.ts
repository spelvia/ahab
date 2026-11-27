/**
 * A session's conversation, built from the agent's events (Stubb's session
 * events, as the server stores and streams them). Pure: no React, no API.
 */

import type {
	AgentChoice,
	AgentFormField,
	AgentQuestion,
	AgentRequest,
	AnswerRequest,
	QuestionAnswer,
} from "@/shared/api/generated/ahab.schemas";
import { formatList, t } from "@/shared/i18n";

export type { AgentChoice, AgentFormField, AgentQuestion, AgentRequest, AnswerRequest };

interface ToolChange {
	path: string;
	kind: { type: "add" | "delete" | "update"; movePath?: string | null };
	diff: string;
}

type ToolDetail =
	| {
			kind: "command";
			command: string;
			cwd?: string | null;
			output?: string | null;
			exitCode?: number | null;
	  }
	| { kind: "fileChange"; changes: ToolChange[] }
	| {
			kind: "notebookEdit";
			path: string;
			cellId?: string | null;
			cellType?: string | null;
			source: string;
			operation?: string | null;
	  }
	| { kind: "mcp"; server: string; tool: string; arguments: unknown; error?: string | null }
	| {
			kind: "mcpCall";
			server: string;
			tool: string;
			arguments?: unknown;
			result?: McpResult | null;
			error?: string | null;
	  }
	| { kind: "other"; name: string; input: unknown; output?: string | null };

/** What an MCP tool returned (Stubb's `McpResult`). */
export interface McpResult {
	/** MCP content blocks: `{ type: "text", text }`, images, resources. */
	content?: { type?: string; text?: string; [field: string]: unknown }[];
	structuredContent?: unknown;
}

export type ToolCallItem = Extract<Item, { type: "toolCall" }>;

export type Item =
	| {
			type: "userMessage";
			id: string;
			content: { type: string; text?: string }[];
			/** The submission it echoes, when Ahab sent it with one. */
			submissionId?: string;
	  }
	| { type: "agentMessage"; id: string; text: string }
	| { type: "thinking"; id: string; text: string; summarized: boolean }
	| { type: "plan"; id: string; text: string }
	| {
			type: "toolCall";
			id: string;
			status: "inProgress" | "completed" | "failed" | "declined";
			durationMs?: number | null;
			detail: ToolDetail;
	  }
	| { type: "other"; id?: string | null; kind: string; raw: unknown };

/** One thing shown in the conversation, in the order it first appeared. */
export type Entry =
	| {
			key: string;
			kind: "item";
			item: Item;
			/** Text streamed so far, before the item ends. */ streamed: string;
			done: boolean;
			/** The kept event that ended it, for showing it from a search. */
			seq?: number;
	  }
	| { key: string; kind: "error"; message: string }
	/** Something Ahab tells about the chat, not an error. */
	| { key: string; kind: "notice"; message: string }
	| ({ key: string; kind: "compaction" } & Compaction)
	| { key: string; kind: "turnEnd"; status: "failed"; error?: string | null }
	| RequestEntry
	| {
			key: string;
			kind: "declined";
			/** The agent's method or tool Stubb declined, such as `item/tool/requestUserInput`. */
			method: string;
			summary?: string | null;
	  };

/** Something the agent asked, where it asked it, and how it ended. */
interface RequestEntry {
	key: string;
	kind: "request";
	request: AgentRequest;
	/** How it stopped being pending; absent while it may still wait. */
	outcome?: RequestOutcome;
}

/** A decision as Stubb reports it in `requestResolved`. */
type AgentDecision =
	| { type: "allow" }
	| { type: "allowForSession" }
	| { type: "deny"; message?: string | null }
	| { type: "cancel" }
	| { type: "answer"; answers: QuestionAnswer[] }
	| { type: "select"; optionId: string }
	| { type: "submit"; values?: Record<string, unknown> | null };

/** How a request ended (Stubb's `Outcome`); other types are a newer Stubb's. */
export type RequestOutcome =
	| { type: "answered"; decision: AgentDecision }
	| { type: "withdrawn" }
	| { type: "unknown" };

/** How full the agent's context is, after its last model call. */
export interface ContextUse {
	/** Tokens the last call used: its input (cached included) and output. */
	used: number;
	/** The model's context window, when the agent reports it. */
	window: number | null;
	/**
	 * The agent compacted its context since, without saying how big it is
	 * now: `used` is from before, until the next call reports again.
	 */
	stale?: boolean;
}

/** A context compaction, as the agent last reported it (Stubb's `Compaction`). */
interface Compaction {
	status: "inProgress" | "completed" | "failed" | "cancelled" | "unknown";
	/** What started it: `manual`, `auto` or `overflow`. */
	trigger?: string | null;
	tokensBefore?: number | null;
	/** Some agents only estimate it. */
	tokensAfter?: number | null;
	error?: string | null;
}

export interface Conversation {
	entries: Entry[];
	/** The highest kept event number seen, to skip repeats. */
	last: number;
	/** A turn is running. */
	working: boolean;
	context?: ContextUse;
	/** The key of the compaction entry still in progress. */
	compacting?: string;
	/** The running turn's ID (its `turnStart`'s `turnId`). */
	turn?: string;
	/** When the running turn started (ms since the epoch), for its timer. */
	turnStartedAt?: number;
	accounting?: { turn: string; tokens?: number; sessionCostUsd?: number };
	/** Finished turns, each after the entry shown last when it ended. */
	finished?: readonly FinishedTurn[];
	/** Its subagents and background tasks, as last told (Stubb's observation). */
	observation?: Observation;
	/** Messages waiting for the running turn to end, the next one first (never kept). */
	queue?: readonly QueuedMessage[];
	/** Each subagent's own events since the chat's stream opened, by its ID. */
	agents?: Readonly<Record<string, readonly StreamEvent[]>>;
}

/** Live events kept for each subagent: the newest, enough for a long run. */
const AGENT_EVENTS = 5_000;

/** A subagent the chat's agent runs (Stubb's `agent::Summary`). */
export interface ObservedAgent {
	origin?: { itemId?: string | null; agentId: string } | null;
	id: string;
	parent: { type: "root" | "agent" | "unknown"; id?: string };
	name?: string | null;
	role?: string | null;
	model?: string | null;
	description?: string | null;
	background?: boolean | null;
	status: string;
}

/** A task the chat's agent started, such as a background command (Stubb's `task::Summary`). */
export interface ObservedTask {
	origin?: { itemId?: string | null; agentId: string } | null;
	progress?: { summary?: string | null; lastToolName?: string | null } | null;
	id: string;
	kind:
		| { type: "command"; command: string }
		| { type: "agentRun"; agentId: string }
		| { type: "other"; name: string };
	description?: string | null;
	background?: boolean | null;
	ambient?: boolean | null;
	status: string;
	/** Whether its output can be read: `complete`, `partial`, `unavailable`, `unknown`. */
	output?: string;
}

export interface Observation {
	canStopAgent?: boolean;
	canStopTask?: boolean;
	rootAgentId?: string;
	agents: ObservedAgent[];
	tasks: ObservedTask[];
}

const ACTIVE = new Set(["pending", "running", "waiting", "paused", "stopping"]);

/** Whether a subagent or task is still at work. */
export function active(status: string): boolean {
	return ACTIVE.has(status);
}

/**
 * What a provider's answer to a stop request (Stubb's `Disposition`) means
 * for the subagent or task: still stopping (asked, accepted, or the agent
 * could not confirm), stopped, already finished, or refused. A newer answer
 * this version does not know keeps waiting for the agent to report it.
 */
export type StopOutcome = "stopping" | "stopped" | "finished" | "rejected";

export function stopOutcome(disposition: string): StopOutcome {
	switch (disposition) {
		case "confirmed":
			return "stopped";
		case "alreadyFinished":
		case "notRunning":
			return "finished";
		case "rejected":
			return "rejected";
		default:
			return "stopping";
	}
}

/** Where stopping one subagent or task stands, shown in its row. */
export type StopState =
	/** Asked: until the agent reports it stopped. `unconfirmed`: the agent could not confirm it. */
	| { phase: "stopping"; unconfirmed?: boolean }
	/** Refused, or the request failed: an error, and Stop again. */
	| { phase: "failed"; message?: string }
	/** Asked, but it still runs after a while: Stop again. */
	| { phase: "timedOut" };

/** The subagents (not the chat's own agent) and the tasks worth showing: not housekeeping. */
export function observed(observation: Observation | undefined): {
	agents: ObservedAgent[];
	tasks: ObservedTask[];
} {
	if (!observation) return { agents: [], tasks: [] };
	return {
		agents: observation.agents.filter(
			(agent) => agent.parent.type !== "root" && agent.id !== observation.rootAgentId,
		),
		// An agent run is its subagent, listed already.
		tasks: observation.tasks.filter((task) => !task.ambient && task.kind.type !== "agentRun"),
	};
}

/** A subagent's name as shown: its own, its role, else "Subagent". */
export function agentName(agent: ObservedAgent): string {
	return agent.name ?? agent.role ?? t("sessions:activity.subagent");
}

/**
 * The subagents `parent` started (`undefined`: the chat's own agent's). A
 * subagent whose parent is not known shows under the chat's own agent.
 */
export function childAgents(
	observation: Observation | undefined,
	parent: string | undefined,
): ObservedAgent[] {
	const { agents } = observed(observation);
	const root = observation?.rootAgentId;
	return agents.filter((agent) =>
		parent === undefined
			? agent.parent.type === "unknown" || !agent.parent.id || agent.parent.id === root
			: agent.parent.type === "agent" && agent.parent.id === parent,
	);
}

/** A subagent's conversation: what is stored of it, then what it did live since. */
export function agentConversation(
	history: readonly StreamEvent["event"][] | undefined,
	live: readonly StreamEvent[] | undefined,
): Conversation {
	const stored = (history ?? []).reduce<Conversation>(
		(conversation, event) => apply(conversation, { seq: null, event }),
		empty,
	);
	return (live ?? []).reduce(apply, stored);
}

/** A finished turn, and the entry it follows. */
export interface FinishedTurn {
	seq: number | null;
	turn: string;
	after: string;
	answer: string;
	/** How long it worked, in ms: as the agent reports it, else from its events' times. */
	durationMs?: number;
	/** When it ended (ms since the epoch), when known. */
	endedAt?: number;
}

/** A message sent while the agent worked, waiting for its turn to end. */
export interface QueuedMessage {
	id: string;
	/** As typed; empty when only files or context were sent. */
	text: string;
	/** How many files go with it. */
	attachments: number;
	/** Sent next, the running turn stopped for it. */
	interrupt: boolean;
	/** A disconnect left its delivery to the agent unconfirmed. */
	unconfirmed?: boolean;
}

export const empty: Conversation = { entries: [], last: 0, working: false };

/** Tokens in a few characters: 950, 38k, 1.2M. */
export function tokens(count: number): string {
	if (count < 1000) return String(count);
	if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
	return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
}

/** An event as the stream sends it: kept events have a number. */
export interface StreamEvent {
	seq: number | null;
	event: { type: string; [field: string]: unknown };
	/** When it was kept, or reached the server (RFC 3339). */
	at?: string;
}

function upsert(
	entries: Entry[],
	key: string,
	change: (entry: Entry | undefined) => Entry,
): Entry[] {
	const index = entries.findIndex((entry) => entry.key === key);
	if (index === -1) return [...entries, change(undefined)];
	const next = entries.slice();
	next[index] = change(entries[index]);
	return next;
}

/** Older Codex adapters preserve web searches and delegated tool calls as native items. */
function presentedItem(item: Item, done: boolean): Item {
	if (item.type !== "other" || !item.id || !item.raw || typeof item.raw !== "object") return item;
	const raw = item.raw as Record<string, unknown>;
	let name: string;
	if (item.kind === "webSearch" && (typeof raw.query === "string" || raw.action))
		name = "WebSearch";
	else if (item.kind === "collabAgentToolCall" && typeof raw.tool === "string") name = "Agent";
	else return item;
	const status =
		raw.status === "failed" || raw.status === "declined"
			? raw.status
			: done
				? "completed"
				: "inProgress";
	return { type: "toolCall", id: item.id, status, detail: { kind: "other", name, input: raw } };
}

/** Applies one event. Repeated kept events (after a reconnect) change nothing. */
export function apply(conversation: Conversation, { seq, event, at }: StreamEvent): Conversation {
	if (seq !== null) {
		if (seq <= conversation.last) return conversation;
		conversation = { ...conversation, last: seq };
	}
	const { entries } = conversation;
	switch (event.type) {
		case "turnAccounting":
			return {
				...conversation,
				accounting: {
					turn: String(event.turnId),
					tokens: typeof event.tokens === "number" ? event.tokens : undefined,
					sessionCostUsd:
						typeof event.sessionCostUsd === "number" ? event.sessionCostUsd : undefined,
				},
			};
		case "turnStart":
			return {
				...conversation,
				working: true,
				turn: typeof event.turnId === "string" ? event.turnId : String(seq ?? ""),
				turnStartedAt: at ? Date.parse(at) : undefined,
			};
		case "itemStart":
		case "itemEnd": {
			const item = presentedItem(event.item as Item, event.type === "itemEnd");
			const key = `item:${item.id ?? `${seq}`}`;
			const done = event.type === "itemEnd";
			return {
				...conversation,
				entries: upsert(entries, key, (entry) => ({
					key,
					kind: "item",
					item:
						done && item.type === "thinking" && !item.text?.trim() && entry?.kind === "item"
							? { ...item, text: itemText(entry) }
							: item,
					streamed: done || entry?.kind !== "item" ? "" : entry.streamed,
					done,
					...(done && seq !== null ? { seq } : {}),
				})),
			};
		}
		case "itemDelta": {
			const key = `item:${String(event.itemId)}`;
			const text = (event.delta as { text?: string } | undefined)?.text ?? "";
			const placeholder: Item = { type: "agentMessage", id: String(event.itemId), text: "" };
			return {
				...conversation,
				entries: upsert(entries, key, (entry) =>
					entry?.kind === "item"
						? entry.done
							? entry
							: { ...entry, streamed: entry.streamed + text }
						: { key, kind: "item", item: placeholder, streamed: text, done: false },
				),
			};
		}
		case "error":
			if (event.retrying) return conversation;
			return {
				...conversation,
				entries: [
					...entries,
					{ key: `error:${seq ?? entries.length}`, kind: "error", message: String(event.message) },
				],
			};
		case "turnEnd": {
			const turn = event.turn as
				| { id?: string; status?: string; error?: string | null; durationMs?: number | null }
				| undefined;
			const status = turn?.status;
			const next = { ...conversation, working: false };
			// Only a failure is noted: a stop someone asked for needs no note.
			if (status === "failed")
				next.entries = [
					...entries,
					{ key: `turnEnd:${turn?.id ?? seq}`, kind: "turnEnd", status, error: turn?.error },
				];
			const ended = conversation.turn ?? turn?.id;
			const lastVisible = next.entries.findLastIndex(isVisibleEntry);
			const after = next.entries[lastVisible]?.key;
			const durationMs =
				turn?.durationMs ??
				(at && conversation.turnStartedAt !== undefined
					? Math.max(0, Date.parse(at) - conversation.turnStartedAt)
					: undefined);
			next.turn = undefined;
			next.turnStartedAt = undefined;
			if (ended && after && !conversation.finished?.some((done) => done.turn === ended))
				next.finished = [
					...(conversation.finished ?? []),
					{
						turn: ended,
						seq,
						after,
						durationMs,
						endedAt: at ? Date.parse(at) : undefined,
						answer: answerEndingAt(next.entries, lastVisible),
					},
				];
			return next;
		}
		case "usage": {
			const usage = event.usage as
				| {
						last?: { input?: number; output?: number; total?: number };
						contextWindow?: number | null;
				  }
				| undefined;
			const last = usage?.last;
			const used = last?.total || (last?.input ?? 0) + (last?.output ?? 0);
			if (!used) return conversation;
			return {
				...conversation,
				context: { used, window: usage?.contextWindow ?? conversation.context?.window ?? null },
			};
		}
		case "compaction": {
			const reported = event.compaction as (Compaction & { id?: string | null }) | undefined;
			if (!reported) return conversation;
			const { id, ...compaction } = reported;
			// Agents that name their compactions pair start and end by name;
			// the others end the one in progress.
			const key = id
				? `compaction:${id}`
				: (conversation.compacting ?? `compaction:${seq ?? entries.length}`);
			const running = compaction.status === "inProgress";
			const context = conversation.context;
			const after = compaction.tokensAfter;
			return {
				...conversation,
				entries: upsert(entries, key, (entry) => ({
					...(entry?.kind === "compaction" ? entry : {}),
					...compaction,
					key,
					kind: "compaction",
				})),
				compacting: running ? key : undefined,
				context:
					compaction.status !== "completed" || !context
						? context
						: typeof after === "number" && after > 0
							? { used: after, window: context.window }
							: { ...context, stale: true },
			};
		}
		case "modelChanged":
			// A switch to another model (Ahab stopped the agent and resumed its
			// session with it): the composer shows the model; no note.
			return { ...conversation, working: false };
		case "agentActivity": {
			const agent = typeof event.agentId === "string" ? event.agentId : undefined;
			const inner = event.event as StreamEvent["event"] | undefined;
			if (!agent || !inner) return conversation;
			const kept = conversation.agents?.[agent] ?? [];
			const next = [...kept, { seq: null, event: inner, at }];
			return {
				...conversation,
				agents: {
					...conversation.agents,
					[agent]: next.length > AGENT_EVENTS ? next.slice(-AGENT_EVENTS) : next,
				},
			};
		}
		case "request": {
			const request = event.request as AgentRequest | undefined;
			if (!request?.id || !request.kind) return conversation;
			const key = `request:${seq ?? request.id}`;
			return {
				...conversation,
				entries: upsert(entries, key, () => ({ key, kind: "request", request })),
			};
		}
		case "requestResolved": {
			const id = event.id;
			const index = entries.findLastIndex(
				(entry) => entry.kind === "request" && entry.request.id === id && !entry.outcome,
			);
			const entry = entries[index];
			if (entry?.kind !== "request") return conversation;
			const next = entries.slice();
			next[index] = { ...entry, outcome: outcomeOf(event.outcome) };
			return { ...conversation, entries: next };
		}
		case "requestDeclined":
			return {
				...conversation,
				entries: [
					...entries,
					{
						key: `declined:${seq ?? entries.length}`,
						kind: "declined",
						method: String(event.method ?? ""),
						summary: typeof event.summary === "string" ? event.summary : null,
					},
				],
			};
		case "modeUnavailable": {
			// Resumed, the agent no longer offered the chat's mode: it runs in its own.
			const mode = String(event.mode ?? "");
			const current = typeof event.current === "string" ? event.current : undefined;
			return {
				...conversation,
				entries: [
					...entries,
					{
						key: `modeUnavailable:${seq ?? entries.length}`,
						kind: "notice",
						message: current
							? t("sessions:conversation.modeUnavailable", { mode, current })
							: t("sessions:conversation.modeUnavailableDefault", { mode }),
					},
				],
			};
		}
		case "queueFailed":
			return {
				...conversation,
				entries: [
					...entries,
					{
						key: `queueFailed:${at ?? entries.length}`,
						kind: "error",
						message: t("sessions:conversation.queueFailed", { error: String(event.message) }),
					},
				],
			};
		case "queue":
			return {
				...conversation,
				queue: Array.isArray(event.items) ? (event.items as QueuedMessage[]) : [],
			};
		case "observation":
			return {
				...conversation,
				observation: {
					...(event.canStopAgent === true && { canStopAgent: true }),
					...(event.canStopTask === true && { canStopTask: true }),
					rootAgentId: typeof event.rootAgentId === "string" ? event.rootAgentId : undefined,
					agents: Array.isArray(event.agents) ? (event.agents as ObservedAgent[]) : [],
					tasks: Array.isArray(event.tasks) ? (event.tasks as ObservedTask[]) : [],
				},
			};
		case "closed": {
			// The chat's state shows it ended; the conversation gets no note.
			// A compaction it never finished ended with it.
			const open = conversation.compacting;
			return {
				...conversation,
				observation: undefined,
				working: false,
				compacting: undefined,
				entries: open
					? upsert(entries, open, (entry) =>
							entry?.kind === "compaction"
								? { ...entry, status: "cancelled" }
								: (entry ?? { key: open, kind: "compaction", status: "cancelled" }),
						)
					: entries,
			};
		}
		default:
			return conversation;
	}
}

function outcomeOf(outcome: unknown): RequestOutcome {
	const value = outcome as { type?: string; decision?: AgentDecision } | undefined;
	if (value?.type === "answered" && value.decision)
		return { type: "answered", decision: value.decision };
	if (value?.type === "withdrawn") return { type: "withdrawn" };
	return { type: "unknown" };
}

/** The text an item shows: its final text, or what has streamed so far. */
export function itemText(entry: Extract<Entry, { kind: "item" }>): string {
	const { item } = entry;
	if (!entry.done && entry.streamed) return entry.streamed;
	switch (item.type) {
		case "agentMessage":
		case "thinking":
		case "plan":
			return item.text;
		case "userMessage":
			// Images have no text; they show from the attached files list.
			return item.content.flatMap((part) => (part.text ? [part.text] : [])).join("\n");
		default:
			return "";
	}
}

/** Providers can report thinking without supplying a displayable summary. */
export function isVisibleEntry(entry: Entry): boolean {
	if (entry.kind !== "item") return true;
	switch (entry.item.type) {
		// Kinds the conversation does not show (Codex reports many).
		case "other":
			return false;
		// Text items show nothing until they have text; a streaming answer
		// shows its cursor.
		case "agentMessage":
			return !entry.done || Boolean(itemText(entry).trim());
		case "thinking":
		case "plan":
			return Boolean(itemText(entry).trim());
		default:
			return true;
	}
}

/**
 * `path` relative to `root` when it is inside it (`src/main.rs`), else as
 * it is: an agent names files by absolute path.
 */
export function relativeTo(root: string | undefined, path: string): string {
	if (!root) return path;
	const base = root.endsWith("/") ? root : `${root}/`;
	return path.startsWith(base) ? path.slice(base.length) : path === root ? "." : path;
}

/** The kind of request, for showing it: a kind this version does not know shows as a permission. */
export type ShownKind = "tool" | "permission" | "question" | "planApproval" | "form";

export function shownKind(request: AgentRequest): ShownKind {
	const type = (request.kind as { type?: string }).type;
	return type === "permission" || type === "question" || type === "planApproval" || type === "form"
		? type
		: "tool";
}

/** A request's tool-like parts: its name, title, description and input; empty for questions, plans and forms. */
function toolOf(request: AgentRequest): {
	name: string;
	displayName?: string | null;
	description?: string | null;
	input: Record<string, unknown>;
	toolCallId?: string | null;
} {
	const kind = request.kind as {
		name?: string;
		displayName?: string | null;
		description?: string | null;
		input?: unknown;
		toolCallId?: string | null;
	};
	const input = kind.input;
	return {
		name: kind.name ?? "",
		displayName: kind.displayName,
		description: kind.description,
		input:
			input && typeof input === "object" && !Array.isArray(input)
				? (input as Record<string, unknown>)
				: {},
		toolCallId: kind.toolCallId,
	};
}

/**
 * What a request asks, in a line: "Write a.rs", "Bash: cargo test", the
 * question, "Approve the plan", the form's message.
 */
export function requestSummary(
	request: AgentRequest,
	shown: (path: string) => string = (path) => path,
): string {
	const { kind } = request;
	switch (kind.type) {
		case "question":
			return kind.questions[0]?.question ?? t("sessions:request.question");
		case "planApproval":
			return t("sessions:request.plan");
		case "form":
			return kind.message || t("sessions:request.form");
	}
	const tool = toolOf(request);
	const name = tool.displayName ?? (tool.name || t("sessions:request.aTool"));
	const { input } = tool;
	const target = input.file_path ?? input.path ?? input.command ?? input.url ?? input.pattern;
	if (tool.description)
		return t("sessions:request.described", { name, description: tool.description });
	const command = commandText(input.command);
	if (command) return t("sessions:request.withTarget", { name, target: command });
	if (typeof target !== "string") return t("sessions:request.use", { name });
	return t("sessions:request.withTarget", { name, target: shown(target) });
}

/** A command as the agent gave it: a line, or its arguments joined. */
function commandText(command: unknown): string | undefined {
	if (typeof command === "string") return command;
	if (Array.isArray(command) && command.every((part) => typeof part === "string"))
		return command.join(" ");
	return undefined;
}

/** What a permission's preview shows, by its tool. */
export type RequestPreview =
	| { type: "command"; command: string; cwd?: string }
	| { type: "diff"; files: { path: string; diff: string }[] }
	| { type: "url"; url: string; prompt?: string }
	| { type: "search"; query: string }
	| { type: "fields"; fields: { key: string; value: string; long: boolean }[] }
	| { type: "none" };

const COMMAND_TOOLS = new Set(["Bash", "commandExecution", "execute", "shell", "exec_command"]);
const EDIT_TOOLS = new Set(["Edit", "MultiEdit", "Write", "fileChange", "edit", "NotebookEdit"]);

/**
 * How to show what a permission is about: its command and folder, its
 * changes as a diff, its address, else its input as readable fields. The
 * tool call it belongs to (`call`) knows more for some agents: Codex's file
 * changes carry their diff there.
 */
export function requestPreview(request: AgentRequest, call?: ToolCallItem): RequestPreview {
	const { name, input } = toolOf(request);
	const detail = call?.detail;
	const command = commandText(input.command);
	if (detail?.kind === "command" || (command && COMMAND_TOOLS.has(name)))
		return {
			type: "command",
			command: command ?? (detail?.kind === "command" ? detail.command : ""),
			cwd:
				typeof input.cwd === "string"
					? input.cwd
					: detail?.kind === "command"
						? (detail.cwd ?? undefined)
						: undefined,
		};
	if (detail?.kind === "fileChange" && detail.changes.length)
		return {
			type: "diff",
			files: detail.changes.map((change) => ({ path: change.path, diff: change.diff })),
		};
	if (EDIT_TOOLS.has(name)) {
		const files = editDiff(input);
		if (files.length) return { type: "diff", files };
	}
	if (typeof input.url === "string")
		return {
			type: "url",
			url: input.url,
			prompt: typeof input.prompt === "string" ? input.prompt : undefined,
		};
	if (name === "WebSearch" && typeof input.query === "string")
		return { type: "search", query: input.query };
	if (command) return { type: "command", command };
	const fields = inputFields(input);
	return fields.length ? { type: "fields", fields } : { type: "none" };
}

/** A tool's input as names and values to read: text as it is, the rest as JSON; empty ones left out. */
export function inputFields(input: unknown): { key: string; value: string; long: boolean }[] {
	if (!input || typeof input !== "object") return [];
	const entries = Array.isArray(input)
		? input.map((value, index): [string, unknown] => [String(index), value])
		: Object.entries(input);
	return entries
		.filter(([, value]) => value !== null && value !== undefined && value !== "")
		.map(([key, value]) => {
			const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
			return { key, value: text, long: text.length > 80 || text.includes("\n") };
		});
}

/** A text's lines, each after `sign`. */
function signed(text: unknown, sign: string): string[] {
	if (typeof text !== "string" || text === "") return [];
	return text
		.replace(/\n$/, "")
		.split("\n")
		.map((line) => `${sign}${line}`);
}

/** An edit as removed lines, then added ones. */
function hunk(before: unknown, after: unknown): string[] {
	return [...signed(before, "-"), ...signed(after, "+")];
}

/** Claude Code's Edit, MultiEdit and Write input as unified diffs. */
function editDiff(input: Record<string, unknown>): { path: string; diff: string }[] {
	const path = typeof input.file_path === "string" ? input.file_path : undefined;
	if (!path) return [];
	if (typeof input.content === "string")
		return [{ path, diff: signed(input.content, "+").join("\n") }];
	if (Array.isArray(input.edits)) {
		const edits = input.edits as { old_string?: unknown; new_string?: unknown }[];
		const lines: string[] = [];
		for (const [index, edit] of edits.entries()) {
			if (index) lines.push("@@");
			lines.push(...hunk(edit.old_string, edit.new_string));
		}
		return [{ path, diff: lines.join("\n") }];
	}
	if ("old_string" in input || "new_string" in input)
		return [{ path, diff: hunk(input.old_string, input.new_string).join("\n") }];
	return [];
}

/** Claude Code's suggested permission updates, which "Allow for this session" applies. */
interface Suggestion {
	type?: string;
	rules?: { toolName?: string; ruleContent?: string | null }[];
	mode?: string;
	directories?: string[];
	destination?: string;
}

function suggestionsOf(request: AgentRequest): Suggestion[] {
	const native = request.native as { permission_suggestions?: unknown } | undefined;
	return Array.isArray(native?.permission_suggestions)
		? (native.permission_suggestions as Suggestion[])
		: [];
}

/**
 * Where Claude Code keeps the rule "Allow for this session" adds, when it
 * outlives the session: `localSettings`, `projectSettings`, `userSettings`.
 */
export function ruleKeptIn(
	request: AgentRequest,
): "localSettings" | "projectSettings" | "userSettings" | undefined {
	const kept = suggestionsOf(request)
		.map((suggestion) => suggestion.destination)
		.find(
			(destination) =>
				destination === "localSettings" ||
				destination === "projectSettings" ||
				destination === "userSettings",
		);
	return kept === "localSettings" || kept === "projectSettings" || kept === "userSettings"
		? kept
		: undefined;
}

/**
 * What "Allow for this session" lets the agent do from then on, when the
 * agent says: Claude Code's suggested rules, a mode, folders.
 */
export function sessionScope(request: AgentRequest): string | undefined {
	const suggestions = suggestionsOf(request);
	const parts = suggestions.flatMap((suggestion): string[] => {
		switch (suggestion.type) {
			case "addRules":
			case "replaceRules":
				return (suggestion.rules ?? []).flatMap((rule) =>
					rule.toolName
						? [rule.ruleContent ? `${rule.toolName}(${rule.ruleContent})` : rule.toolName]
						: [],
				);
			case "setMode":
				return suggestion.mode === "acceptEdits" ? [t("sessions:permission.scope.edits")] : [];
			case "addDirectories":
				return (suggestion.directories ?? []).map((directory) =>
					t("sessions:permission.scope.folder", { folder: directory }),
				);
			default:
				return [];
		}
	});
	if (parts.length) return formatList(parts);
	const { name } = toolOf(request);
	if (name === "commandExecution") return t("sessions:permission.scope.command");
	if (name === "fileChange") return t("sessions:permission.scope.fileChanges");
	return undefined;
}

/** The agent's own choice that a plain decision stands for: the first of its kind. */
export function choiceOf(
	options: readonly AgentChoice[],
	kinds: readonly AgentChoice["kind"][],
): AgentChoice | undefined {
	for (const kind of kinds) {
		const found = options.find((option) => option.kind === kind);
		if (found) return found;
	}
	return undefined;
}

/** An answer being written to a question: the options picked, and your own words. */
export interface Draft {
	selected: string[];
	other: string;
	/** "Other…" is picked (a single-choice question's own answer). */
	otherOn: boolean;
}

export const emptyDraft: Draft = { selected: [], other: "", otherOn: false };

/** Whether a question's draft answers it. */
export function answered(question: AgentQuestion, draft: Draft | undefined): boolean {
	if (!draft) return false;
	if (draft.otherOn || question.options.length === 0) return Boolean(draft.other.trim());
	return draft.selected.length > 0;
}

/** Picks or unpicks an option, as the question allows one or several. */
export function toggleOption(question: AgentQuestion, draft: Draft, label: string): Draft {
	if (question.multiSelect)
		return {
			...draft,
			selected: draft.selected.includes(label)
				? draft.selected.filter((each) => each !== label)
				: [...draft.selected, label],
		};
	return { ...draft, selected: [label], otherOn: false };
}

/** Picks or unpicks "Other…": on its own for a single choice, beside the options for several. */
export function toggleOther(question: AgentQuestion, draft: Draft): Draft {
	const otherOn = !draft.otherOn;
	return question.multiSelect ? { ...draft, otherOn } : { ...draft, otherOn, selected: [] };
}

/** The answer to send: one per question, from the drafts. */
export function answerFrom(
	questions: readonly AgentQuestion[],
	drafts: Readonly<Record<string, Draft>>,
): AnswerRequest {
	return {
		decision: "answer",
		answers: questions.map((question) => {
			const draft = drafts[question.id] ?? emptyDraft;
			const other =
				(draft.otherOn || question.options.length === 0) && draft.other.trim()
					? draft.other.trim()
					: undefined;
			return {
				questionId: question.id,
				selected: draft.selected,
				...(other ? { other } : {}),
			};
		}),
	};
}

/** A form's values to start from: each field's default. */
export function formDefaults(fields: readonly AgentFormField[]): Record<string, unknown> {
	const values: Record<string, unknown> = {};
	for (const field of fields) {
		if (field.default !== undefined && field.default !== null) values[field.name] = field.default;
		else if (field.kind === "boolean") values[field.name] = false;
		else if (field.kind === "multiSelect") values[field.name] = [];
	}
	return values;
}

/**
 * What is wrong with a field's value, if anything: missing when required,
 * not a number, outside its bounds. Empty optional fields are left out.
 */
export function fieldProblem(field: AgentFormField, value: unknown): string | undefined {
	const blank =
		value === undefined ||
		value === null ||
		value === "" ||
		(Array.isArray(value) && value.length === 0);
	if (blank) return field.required ? t("sessions:form.required") : undefined;
	const { minimum: min, maximum: max } = field;
	switch (field.kind) {
		case "number":
		case "integer": {
			const number = Number(value);
			if (!Number.isFinite(number)) return t("sessions:form.notNumber");
			if (field.kind === "integer" && !Number.isInteger(number))
				return t("sessions:form.notInteger");
			if (typeof min === "number" && number < min) return t("sessions:form.atLeast", { min });
			if (typeof max === "number" && number > max) return t("sessions:form.atMost", { max });
			return undefined;
		}
		case "text": {
			const length = String(value).length;
			if (typeof min === "number" && length < min)
				return t("sessions:form.minLength", { count: min });
			if (typeof max === "number" && length > max)
				return t("sessions:form.maxLength", { count: max });
			return undefined;
		}
		case "multiSelect": {
			const count = Array.isArray(value) ? value.length : 0;
			if (typeof min === "number" && count < min)
				return t("sessions:form.pickAtLeast", { count: min });
			if (typeof max === "number" && count > max)
				return t("sessions:form.pickAtMost", { count: max });
			return undefined;
		}
		default:
			return undefined;
	}
}

/** The values to submit: numbers as numbers, empty optional fields left out. */
export function formValues(
	fields: readonly AgentFormField[],
	values: Readonly<Record<string, unknown>>,
): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	for (const field of fields) {
		const value = values[field.name];
		if (value === undefined || value === null || value === "") continue;
		out[field.name] = field.kind === "number" || field.kind === "integer" ? Number(value) : value;
	}
	return out;
}

/**
 * What was chosen, in a line, for an answered request: "Allowed", the
 * option picked, "Denied: too broad", each answer.
 */
export function outcomeSummary(request: AgentRequest, outcome: RequestOutcome | undefined): string {
	if (!outcome) return t("sessions:outcome.noAnswer");
	if (outcome.type === "withdrawn") return t("sessions:outcome.withdrawn");
	if (outcome.type !== "answered") return t("sessions:outcome.answered");
	const { decision } = outcome;
	const kind = shownKind(request);
	switch (decision.type) {
		case "allow":
			return kind === "planApproval"
				? t("sessions:outcome.approved")
				: t("sessions:outcome.allowed");
		case "allowForSession":
			return t("sessions:outcome.allowedForSession");
		case "deny":
			if (kind === "planApproval")
				return decision.message
					? t("sessions:outcome.keptPlanningWith", { message: decision.message })
					: t("sessions:outcome.keptPlanning");
			if (kind === "form" || kind === "question") return t("sessions:outcome.declined");
			return decision.message
				? t("sessions:outcome.deniedWith", { message: decision.message })
				: t("sessions:outcome.denied");
		case "cancel":
			return t("sessions:outcome.cancelled");
		case "select": {
			const options =
				request.kind.type === "permission" || request.kind.type === "planApproval"
					? request.kind.options
					: [];
			const label = options.find((option) => option.id === decision.optionId)?.label;
			return label ?? decision.optionId;
		}
		case "submit":
			return t("sessions:outcome.submitted");
		case "answer":
			return t("sessions:outcome.answered");
		default:
			return t("sessions:outcome.answered");
	}
}

/** Each question's answer as shown once given: the options picked, then your words (masked when secret). */
export function shownAnswers(
	request: AgentRequest,
	outcome: RequestOutcome | undefined,
): { question: AgentQuestion; answer: string }[] {
	if (request.kind.type !== "question") return [];
	const answers =
		outcome?.type === "answered" && outcome.decision.type === "answer"
			? outcome.decision.answers
			: [];
	return request.kind.questions.flatMap((question) => {
		const given = answers.find((answer) => answer.questionId === question.id);
		if (!given) return [];
		const other = given.other?.trim()
			? question.secret
				? "••••••"
				: given.other.trim()
			: undefined;
		const parts = [...(given.selected ?? []), ...(other ? [other] : [])];
		return parts.length ? [{ question, answer: formatList(parts) }] : [];
	});
}

/** A reasoning effort's name for people: "xhigh" → "Extra high". */
export function effortLabel(effort: string): string {
	if (isKnownEffort(effort)) return t(`sessions:effort.${effort}`);
	return effort.charAt(0).toUpperCase() + effort.slice(1);
}

const EFFORTS = ["xhigh", "high", "medium", "low", "minimal", "off"] as const;

function isKnownEffort(effort: string): effort is (typeof EFFORTS)[number] {
	return (EFFORTS as readonly string[]).includes(effort);
}

/** A permission mode's ID, as the server names it. */
type PermissionModeId =
	| "plan"
	| "ask"
	| "acceptEdits"
	| "bypass"
	| "readOnly"
	| "workspace"
	| "fullAccess";

/**
 * What an agent may do without asking. Its label and description are
 * `permissionModes.<id>` in the `sessions` texts.
 */
export interface PermissionMode {
	id: PermissionModeId;
	/** Nothing is asked and anything may change. */
	risky?: boolean;
}

const MODES: Record<string, PermissionMode[]> = {
	"claude-code": [
		{ id: "plan" },
		{ id: "ask" },
		{ id: "acceptEdits" },
		{ id: "bypass", risky: true },
	],
	codex: [
		{ id: "readOnly" },
		{ id: "ask" },
		{ id: "workspace" },
		{ id: "fullAccess", risky: true },
	],
};

/** An agent's permission modes; Pi has none. */
export function permissionModes(agent: string | undefined): PermissionMode[] {
	return (agent && MODES[agent]) || [];
}

/** The mode an agent starts in unless another is chosen. */
export function defaultMode(agent: string | undefined): string | undefined {
	if (agent === "claude-code") return "ask";
	if (agent === "codex") return "workspace";
	return undefined;
}

/** "4s", "2m 5s", "1h 3m": how long a turn worked. */
export function workedFor(ms: number): string {
	const seconds = Math.max(0, Math.round(ms / 1000));
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = seconds % 60;
	if (h) return `${h}h ${m}m`;
	if (m) return `${m}m ${s}s`;
	return `${s}s`;
}

/** The conversation as Markdown, to copy: your messages and the agent's answers, in order. */
export function transcript(entries: Entry[], agent: string, you: string): string {
	return `${messages(entries, agent, you).join("\n\n")}\n`;
}

/** The most a fork's chat history carries, in bytes: under the server's 256 KiB for one reference. */
const HISTORY_BYTES = 250 * 1024;

/**
 * The conversation up to the entry `after` (where a finished turn ends), as a
 * fork carries it to a new chat with any agent: your messages and the
 * agent's, as Markdown. The earliest make way, `cut` said in their place,
 * when it would be too long.
 */
export function historyUpTo(
	entries: Entry[],
	after: string,
	agent: string,
	you: string,
	cut: string,
): string {
	const end = entries.findIndex((entry) => entry.key === after);
	const parts = messages(end === -1 ? entries : entries.slice(0, end + 1), agent, you);
	const encoder = new TextEncoder();
	let size = parts.reduce((total, part) => total + encoder.encode(part).length + 2, 0);
	let dropped = 0;
	while (size > HISTORY_BYTES && dropped < parts.length - 1) {
		size -= encoder.encode(parts[dropped] ?? "").length + 2;
		dropped += 1;
	}
	const kept = parts.slice(dropped);
	return `${(dropped ? [cut, ...kept] : kept).join("\n\n")}\n`;
}

/** Your messages and the agent's, each under its heading. */
function messages(entries: Entry[], agent: string, you: string): string[] {
	const parts: string[] = [];
	for (const entry of entries) {
		if (entry.kind !== "item") continue;
		const { item } = entry;
		if (item.type === "userMessage") {
			const text = itemText(entry).trim();
			if (text) parts.push(`## ${you}\n\n${text}`);
		} else if (item.type === "agentMessage") {
			const text = itemText(entry).trim();
			if (text) parts.push(`## ${agent}\n\n${text}`);
		}
	}
	return parts;
}

/** The agent's answer in the turn that ends at `after`: its messages since your last one. */
export function turnAnswer(entries: Entry[], after: string): string {
	return answerEndingAt(
		entries,
		entries.findIndex((entry) => entry.key === after),
	);
}

function answerEndingAt(entries: Entry[], end: number): string {
	const said: string[] = [];
	for (let at = end; at >= 0; at -= 1) {
		const entry = entries[at];
		if (entry?.kind !== "item") continue;
		if (entry.item.type === "userMessage") break;
		if (entry.item.type === "agentMessage") said.push(itemText(entry).trim());
	}
	return said.toReversed().filter(Boolean).join("\n\n");
}

/** Where each turn starts: the keys of your messages, in order (the turn rail). */
export function turnStarts(entries: Entry[]): { key: string; text: string; seq?: number }[] {
	return entries.flatMap((entry) =>
		entry.kind === "item" && entry.item.type === "userMessage"
			? [{ key: entry.key, text: itemText(entry), seq: entry.seq }]
			: [],
	);
}

/** A turn on the rail: where its message is in the history, and its text. */
export interface RailTurn {
	seq: number;
	text: string;
}

/**
 * Every turn of the chat, for the rail: the server's list (the whole
 * history, loaded or not) with the loaded ones it does not have yet, such
 * as a message just sent, in history order.
 */
export function railTurns(
	all: readonly RailTurn[] | undefined,
	loaded: readonly { seq?: number; text: string }[],
): RailTurn[] {
	const bySeq = new Map((all ?? []).map((turn) => [turn.seq, turn]));
	for (const turn of loaded)
		if (turn.seq !== undefined && !bySeq.has(turn.seq))
			bySeq.set(turn.seq, { seq: turn.seq, text: turn.text });
	return [...bySeq.values()].toSorted((a, b) => a.seq - b.seq);
}

/**
 * Whether the conversation holds the agent's echo of submission `id`, or
 * the queue holds it: either way it no longer shows as "sending".
 */
export function hasArrived(
	entries: readonly Entry[],
	queue: readonly QueuedMessage[] | undefined,
	id: string,
): boolean {
	return (
		Boolean(queue?.some((item) => item.id === id)) ||
		entries.some(
			(entry) =>
				entry.kind === "item" &&
				entry.item.type === "userMessage" &&
				entry.item.submissionId === id,
		)
	);
}

/** A file a turn changed: what happened to it, and its lines added and removed. */
export interface TurnChange {
	path: string;
	kind: "add" | "update" | "delete";
	/** Whether it is still there to open. */
	exists: boolean;
	added: number;
	removed: number;
}

/** Lines a unified diff adds and removes (its file headers aside). */
export function diffStat(diff: string): { added: number; removed: number } {
	let added = 0;
	let removed = 0;
	for (const line of diff.split("\n")) {
		if (line.startsWith("+++") || line.startsWith("---")) continue;
		if (line.startsWith("+")) added += 1;
		else if (line.startsWith("-")) removed += 1;
	}
	return { added, removed };
}

/**
 * Adds one change to a turn's: a file changed twice is one row, its lines
 * summed; created then edited stays created, and deleted last is deleted.
 */
export function addTurnChange(
	changes: Map<string, TurnChange>,
	change: { path: string; kind: TurnChange["kind"]; diff?: string | null },
) {
	const { added, removed } = diffStat(change.diff ?? "");
	const before = changes.get(change.path);
	changes.set(change.path, {
		path: change.path,
		kind: change.kind === "delete" ? "delete" : before?.kind === "add" ? "add" : change.kind,
		exists: change.kind !== "delete",
		added: (before?.added ?? 0) + added,
		removed: (before?.removed ?? 0) + removed,
	});
}
