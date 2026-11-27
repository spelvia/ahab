import { cn } from "cn";
import {
	Brain,
	ChevronRight,
	CircleAlert,
	FileDiff,
	FoldVertical,
	Info,
	ListTodo,
	LoaderCircle,
	Plug,
	ShieldOff,
	SquareTerminal,
	Wrench,
} from "lucide-react";
import {
	type ComponentProps,
	createContext,
	memo,
	type ReactNode,
	startTransition,
	useCallback,
	useContext,
	useEffect,
	useEffectEvent,
	useMemo,
	useRef,
	useState,
} from "react";

import { AttachedFiles, splitAttachments } from "@/modules/sessions/attachment";
import { ChatRoot, useShownPath } from "@/modules/sessions/chat/components/chat-root";
import { shellSummary, unwrapShell } from "@/modules/sessions/chat/components/command-text";
import { Diff } from "@/modules/sessions/chat/components/diff-block";
import { McpCallBody } from "@/modules/sessions/chat/components/mcp-call";
import { PlanProgress, stepsOf } from "@/modules/sessions/chat/components/plan-progress";
import { RequestCard } from "@/modules/sessions/chat/components/request-card";
import {
	CommandBody,
	CreatedCode,
	NotebookCell,
	OpenChange,
	OtherBody,
	ToolFile,
	toolTarget,
} from "@/modules/sessions/chat/components/tool-detail";
import { TurnChanges } from "@/modules/sessions/chat/components/turn-changes";
import { TurnFooter } from "@/modules/sessions/chat/components/turn-footer";
import { useRevealedMessage } from "@/modules/sessions/chat/hooks";
import {
	type Entry,
	addTurnChange,
	type FinishedTurn,
	type Item,
	isVisibleEntry,
	itemText,
	type ToolCallItem,
	type TurnChange,
	tokens as tokenCount,
} from "@/modules/sessions/chat/model";
import { SentReferences, splitReferences } from "@/modules/sessions/reference";
import { useAppearance } from "@/shared/appearance";
import { AfterTurn } from "@/shared/chat-context/after-turn";
import type { ContextReference } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";
import { Markdown } from "@/shared/ui/markdown";

type Resend = (text: string, files: string[]) => Promise<void>;
const ResendMessage = createContext<Resend | undefined>(undefined);

/** A session's conversation, in the order things happened. */
export function ConversationView({
	entries,
	onResend,
	sideWork,
	onOpenRelated,
	projectId,
	onOpenFile,
	chatId,
	finished,
	onAddContext,
	forkOptions,
	requests,
}: {
	entries: Entry[];
	onResend?: Resend;
	sideWork?: ReadonlyMap<string, { id: string; type: "agent" | "task"; label: string }[]>;
	onOpenRelated?: (type: "agent" | "task", id: string) => void;
	/** Where attached files are read from. */
	projectId?: string;
	/** Context chips open their file at its lines. */
	onOpenFile?: (path: string, line?: number) => void;
	/** The chat, for what modules show after its finished turns. */
	chatId?: string;
	finished?: readonly FinishedTurn[];
	/** Puts context in this chat's composer. */
	onAddContext?: (references: ContextReference[]) => void;
	/** Fork's choices for the finished turn ending at entry `after`; absent where it cannot be forked. */
	forkOptions?: (after: string, close: () => void) => ReactNode;
	/** Answering what the agent asks: the chat, the agent's name, and which requests wait now. */
	requests?: { chatId: string; agent: string; open: ReadonlySet<string> };
}) {
	const list = useRef<HTMLOListElement>(null);
	const observed = useRef(new Map<Element, (shown: boolean) => void>());
	const observer = useRef<IntersectionObserver>(null);
	const observe = useCallback((row: HTMLLIElement, change: (shown: boolean) => void) => {
		observed.current.set(row, change);
		observer.current?.observe(row);
		return () => {
			observer.current?.unobserve(row);
			observed.current.delete(row);
		};
	}, []);
	useEffect(() => {
		const root = list.current?.closest("[data-chat-scroll]");
		if (!root || typeof IntersectionObserver === "undefined") return;
		const mounted = new IntersectionObserver(
			(updates) => {
				for (const update of updates) {
					observed.current.get(update.target)?.(update.isIntersecting);
				}
			},
			{ root, rootMargin: "800px 0px" },
		);
		observer.current = mounted;
		for (const row of observed.current.keys()) mounted.observe(row);
		return () => {
			mounted.disconnect();
			observer.current = null;
		};
	}, []);
	// A question or plan shows as its request: the tool call that asked is not shown too.
	const { calls, asked } = useMemo(() => {
		const byId = new Map<string, ToolCallItem>();
		const askers = new Set<string>();
		for (const entry of entries) {
			if (entry.kind === "item" && entry.item.type === "toolCall")
				byId.set(entry.item.id, entry.item);
			if (entry.kind === "request") {
				const { kind } = entry.request;
				if ((kind.type === "question" || kind.type === "planApproval") && kind.toolCallId)
					askers.add(kind.toolCallId);
			}
		}
		return { calls: byId, asked: askers };
	}, [entries]);
	const visible = useMemo(
		() =>
			entries.filter(
				(entry) =>
					isVisibleEntry(entry) &&
					!(entry.kind === "item" && entry.item.type === "toolCall" && asked.has(entry.item.id)),
			),
		[entries, asked],
	);
	useRevealedMessage(chatId, list, entries);
	const finishedByEntry = useMemo(() => {
		const groups = new Map<string, FinishedTurn[]>();
		for (const turn of finished ?? []) {
			const turns = groups.get(turn.after);
			if (turns) turns.push(turn);
			else groups.set(turn.after, [turn]);
		}
		return groups;
	}, [finished]);
	const rows = useMemo(() => {
		const groups: Entry[][] = [];
		let reads: Entry[] | undefined;
		for (const entry of visible) {
			const read =
				entry.kind === "item" &&
				entry.item.type === "toolCall" &&
				entry.item.detail.kind === "other" &&
				entry.item.detail.name === "Read" &&
				entry.item.status === "completed";
			if (read && reads) reads.push(entry);
			else {
				const group = [entry];
				groups.push(group);
				reads = read ? group : undefined;
			}
			if (finishedByEntry.has(entry.key)) reads = undefined;
		}
		return groups;
	}, [visible, finishedByEntry]);
	const turnDetails = useMemo(() => {
		const result = new Map<string, { message?: string; changes: TurnChange[] }>();
		let message: string | undefined;
		let changes = new Map<string, TurnChange>();
		for (const row of entries) {
			if (row.kind === "item" && row.item.type === "userMessage") {
				const original = itemText(row);
				// Older command echoes have no stable skill or MCP invocation ID.
				message = original.startsWith("/") ? undefined : original;
				changes = new Map();
			}
			if (
				row.kind === "item" &&
				row.item.type === "toolCall" &&
				row.item.status === "completed" &&
				row.item.detail.kind === "fileChange"
			) {
				for (const file of row.item.detail.changes)
					addTurnChange(changes, {
						path: (file.kind.type === "update" && file.kind.movePath) || file.path,
						kind: file.kind.type,
						diff: file.diff,
					});
			}
			if (
				row.kind === "item" &&
				row.item.type === "toolCall" &&
				row.item.status === "completed" &&
				row.item.detail.kind === "notebookEdit"
			)
				addTurnChange(changes, { path: row.item.detail.path, kind: "update" });
			if (row.kind === "turnEnd" || finishedByEntry.has(row.key)) {
				result.set(row.key, { message, changes: [...changes.values()] });
				message = undefined;
				changes = new Map();
			}
		}
		return result;
	}, [entries, finishedByEntry]);
	const { t } = useTranslation("sessions");
	return (
		<ResendMessage value={onResend}>
			<ol ref={list} className="chat-transcript flex flex-col">
				{rows.map((group, index) => {
					const entry = group[group.length - 1]!;
					return (
						<TranscriptRow
							key={entry.key}
							entry={entry}
							observe={observe}
							recent={rows.length - index <= 24}
						>
							{group.length > 1 ? (
								<Fold icon={<Wrench />} title={t("tools.readFiles", { count: group.length })}>
									{group.map((read) => (
										<div key={read.key} data-seq={read.kind === "item" ? read.seq : undefined}>
											<EntryView entry={read} onOpenFile={onOpenFile} projectId={projectId} />
										</div>
									))}
								</Fold>
							) : (
								<EntryView
									entry={entry}
									projectId={projectId}
									onOpenFile={onOpenFile}
									chatId={requests?.chatId}
									agent={requests?.agent}
									pending={
										entry.kind === "request" &&
										!entry.outcome &&
										Boolean(requests?.open.has(entry.request.id))
									}
									call={entry.kind === "request" ? requestCall(entry.request, calls) : undefined}
								/>
							)}
							{entry.kind === "turnEnd" && onResend && turnDetails.get(entry.key)?.message && (
								<Button
									size="sm"
									variant="ghost"
									onClick={() => {
										const original = splitAttachments(turnDetails.get(entry.key)?.message ?? "");
										void onResend(
											original.text,
											original.files.map((file) => file.id),
										).catch(() => {});
									}}
								>
									{t("tools.retryTurn")}
								</Button>
							)}
							{entry.kind === "item" &&
								entry.item.type === "toolCall" &&
								sideWork?.get(entry.item.id)?.map((action) => (
									<button
										type="button"
										key={action.type + ":" + action.id}
										className="text-muted-foreground hover:text-foreground ml-8 text-xs hover:underline"
										onClick={() => onOpenRelated?.(action.type, action.id)}
									>
										{action.label}
									</button>
								))}
							{chatId
								? finishedByEntry.get(entry.key)?.map((turn) => (
										<div key={turn.turn} className="mt-2 flex flex-col gap-2">
											<AfterTurn
												chatId={chatId}
												turnId={turn.turn}
												projectId={projectId}
												onAddContext={onAddContext}
												onOpenFile={onOpenFile}
											/>
											<TurnChanges
												changes={turnDetails.get(entry.key)?.changes ?? []}
												onOpenFile={onOpenFile}
											/>
											<TurnFooter
												answer={turn.answer}
												durationMs={turn.durationMs}
												endedAt={turn.endedAt}
												forkOptions={forkOptions ? forkOptions.bind(null, turn.after) : undefined}
											/>
										</div>
									))
								: null}
						</TranscriptRow>
					);
				})}
			</ol>
		</ResendMessage>
	);
}

/**
 * Keep the row's position for search, the turn rail and scroll anchoring,
 * and release its heavy children when it is far outside the scrollport.
 */
function TranscriptRow({
	entry,
	observe,
	recent,
	children,
}: {
	entry: Entry;
	observe: (row: HTMLLIElement, change: (shown: boolean) => void) => () => void;
	recent: boolean;
	children: ReactNode;
}) {
	const [near, setNear] = useState(false);
	const [height, setHeight] = useState(0);
	const mounted = recent || near || typeof IntersectionObserver === "undefined";
	const track = useCallback(
		(row: HTMLLIElement | null) => {
			if (!row) return;
			return observe(row, (shown) => {
				if (!shown) {
					const selection = row.ownerDocument.getSelection();
					if (
						row.matches(":focus-within") ||
						row.querySelector("details[open], [aria-expanded=true]") ||
						(selection &&
							!selection.isCollapsed &&
							(row.contains(selection.anchorNode) || row.contains(selection.focusNode)))
					)
						return;
					const measured = row.getBoundingClientRect().height;
					if (measured > 0) setHeight(measured);
				}
				setNear(shown);
			});
		},
		[observe],
	);
	return (
		<li
			ref={track}
			data-seq={entry.kind === "item" ? entry.seq : undefined}
			data-entry={entry.key}
			data-kind={entry.kind === "item" ? entry.item.type : entry.kind}
			style={mounted ? undefined : { minHeight: height || 96 }}
			className="data-[revealed]:ring-primary/60 data-[revealed]:bg-primary/5 rounded-lg transition-[box-shadow,background-color] duration-500 data-[revealed]:ring-2"
		>
			{mounted ? children : null}
		</li>
	);
}

/**
 * One entry; kept entries keep their identity as the conversation grows,
 * so only the one changing re-renders (and re-reads its markdown).
 */
const EntryView = memo(function EntryView({
	entry,
	projectId,
	onOpenFile,
	chatId,
	agent,
	pending,
	call,
}: {
	entry: Entry;
	projectId?: string;
	onOpenFile?: (path: string, line?: number) => void;
	chatId?: string;
	agent?: string;
	/** A request that waits for an answer now. */
	pending?: boolean;
	/** A request's tool call. */
	call?: ToolCallItem;
}) {
	const { t } = useTranslation("sessions");
	switch (entry.kind) {
		case "request":
			// A subagent's requests are answered in its parent's chat.
			if (!chatId) return null;
			return (
				<RequestCard
					chatId={chatId}
					request={entry.request}
					outcome={entry.outcome}
					pending={Boolean(pending)}
					agent={agent ?? ""}
					call={call}
				/>
			);
		case "declined":
			return (
				<p
					className="text-muted-foreground flex items-center gap-2 px-2 text-xs"
					title={t("declined.why")}
				>
					<ShieldOff className="size-3.5 shrink-0" aria-hidden />
					<span className="min-w-0 truncate">
						{entry.summary || entry.method
							? t("declined.row", { what: entry.summary || entry.method })
							: t("declined.rowPlain")}
					</span>
				</p>
			);
		case "error":
			return (
				<p className="text-destructive flex items-start gap-2 text-sm">
					<CircleAlert className="mt-0.5 size-4 shrink-0" />
					{entry.message}
					<CopyButton text={entry.message} label={t("tools.error")} />
				</p>
			);
		case "notice":
			return (
				<p className="text-muted-foreground flex items-start gap-2 text-sm">
					<Info className="mt-0.5 size-4 shrink-0" aria-hidden />
					{entry.message}
				</p>
			);
		case "compaction":
			return <CompactionView entry={entry} />;
		case "turnEnd":
			return (
				<p className="text-muted-foreground text-sm">
					{entry.error
						? t("conversation.turnFailedWith", { error: entry.error })
						: t("conversation.turnFailed")}
					{entry.error && <CopyButton text={entry.error} label={t("tools.error")} />}
				</p>
			);
		case "item":
			return <ItemView entry={entry} projectId={projectId} onOpenFile={onOpenFile} />;
	}
});

const STREAM_RENDER_INTERVAL = 80;

/** Sample the growing source without delaying the final, fully formatted message. */
function AgentMarkdown({
	text,
	streaming,
	imageBase,
	files,
	className,
}: ComponentProps<typeof Markdown>) {
	const [sample, setSample] = useState(text);
	const update = useEffectEvent(() => startTransition(() => setSample(text)));
	const behind = Boolean(streaming && sample !== text);
	useEffect(() => {
		if (!behind) return;
		const timer = setTimeout(() => update(), STREAM_RENDER_INTERVAL);
		return () => clearTimeout(timer);
	}, [behind]);
	const shown = streaming ? sample : text;
	return useMemo(
		() => (
			<Markdown
				text={shown}
				streaming={streaming}
				imageBase={imageBase}
				files={files}
				className={className}
			/>
		),
		[shown, streaming, imageBase, files, className],
	);
}

/** A divider where the agent compacted its context. */
function CompactionView({ entry }: { entry: Extract<Entry, { kind: "compaction" }> }) {
	const { t } = useTranslation("sessions");
	const { tokensBefore: before, tokensAfter: after } = entry;
	const label =
		entry.status === "inProgress"
			? t("conversation.compacting")
			: entry.status === "failed"
				? entry.error
					? t("conversation.compactionFailedWith", { error: entry.error })
					: t("conversation.compactionFailed")
				: entry.status === "cancelled"
					? t("conversation.compactionCancelled")
					: typeof before === "number" && typeof after === "number"
						? t("conversation.compactedFromTo", {
								before: tokenCount(before),
								after: tokenCount(after),
							})
						: t("conversation.compacted");
	const by =
		entry.trigger === "manual" || entry.trigger === "auto" || entry.trigger === "overflow"
			? t(`conversation.compactionTrigger_${entry.trigger}`)
			: undefined;
	return (
		<div
			role="separator"
			className={cn(
				"flex items-center gap-3 text-xs",
				entry.status === "failed" ? "text-destructive" : "text-muted-foreground",
			)}
		>
			<span className="bg-border h-px flex-1" />
			<span className="flex items-center gap-1.5">
				{entry.status === "inProgress" ? (
					<LoaderCircle className="size-3.5 motion-safe:animate-spin" />
				) : (
					<FoldVertical className="size-3.5" />
				)}
				{label}
				{by ? <span className="opacity-70">· {by}</span> : null}
			</span>
			<span className="bg-border h-px flex-1" />
		</div>
	);
}

function ItemView({
	entry,
	projectId,
	onOpenFile,
}: {
	entry: Extract<Entry, { kind: "item" }>;
	projectId?: string;
	onOpenFile?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation("sessions");
	const look = useAppearance();
	const root = useContext(ChatRoot);
	const imageBase = useMemo(
		() => (projectId ? { project: projectId, path: "", root } : undefined),
		[projectId, root],
	);
	const files = useMemo(
		() => (onOpenFile ? { root, open: onOpenFile } : undefined),
		[root, onOpenFile],
	);
	const resend = useContext(ResendMessage);
	const { item } = entry;
	const text = itemText(entry);
	const steps = stepsOf(entry);
	if (steps?.length) return <PlanProgress steps={steps} />;
	switch (item.type) {
		case "userMessage": {
			const sent = splitAttachments(text);
			const own = splitReferences(sent.text);
			return (
				<div className="group/message flex flex-col items-end gap-1.5">
					{projectId ? <AttachedFiles projectId={projectId} files={sent.files} /> : null}
					<UserText
						text={own.text}
						onSend={
							resend && !text.startsWith("/")
								? (value) =>
										resend(
											value + sent.text.slice(own.text.length),
											sent.files.map((file) => file.id),
										)
								: undefined
						}
					/>
					<SentReferences references={own.references} onOpenFile={onOpenFile} />
				</div>
			);
		}
		case "agentMessage":
			return (
				<div className="text-foreground">
					<AgentMarkdown imageBase={imageBase} files={files} text={text} streaming={!entry.done} />
					{!entry.done ? (
						<span className="bg-primary mt-1 inline-block h-4 w-1.5 motion-safe:animate-pulse" />
					) : null}
				</div>
			);
		case "thinking":
			return (
				<Fold
					icon={<Brain />}
					title={t("conversation.thinking")}
					dim
					defaultOpen={look.expandThinking}
				>
					<AgentMarkdown
						imageBase={imageBase}
						files={files}
						text={text}
						streaming={!entry.done}
						className="text-muted-foreground"
					/>
				</Fold>
			);
		case "plan":
			return (
				<div className="border-border rounded-lg border p-3">
					<p className="text-muted-foreground mb-1 flex items-center gap-1.5 text-xs font-medium">
						<ListTodo className="size-3.5" />
						{t("conversation.plan")}
					</p>
					<Markdown imageBase={imageBase} files={files} text={text} />
				</div>
			);
		case "toolCall":
			return <ToolCallView item={item} onOpenFile={onOpenFile} />;
		default:
			return null;
	}
}

function UserText({ text, onSend }: { text: string; onSend?: (value: string) => Promise<void> }) {
	const { t } = useTranslation("sessions");
	const [edit, setEdit] = useState<string>();
	const [expanded, setExpanded] = useState(false);
	const [busy, setBusy] = useState(false);
	const send = async (value: string) => {
		if (!onSend || busy) return;
		setBusy(true);
		try {
			await onSend(value);
			setEdit(undefined);
		} finally {
			setBusy(false);
		}
	};
	const long = text.length > 1600;
	return (
		<>
			{edit !== undefined ? (
				<textarea
					aria-label={t("tools.editMessage")}
					className="bg-card w-full rounded-md border p-2 text-sm"
					value={edit}
					onChange={(event) => setEdit(event.target.value)}
				/>
			) : (
				text && (
					<p className="bg-card text-heading max-w-[85%] rounded-xl px-3.5 py-2.5 text-sm break-words whitespace-pre-wrap">
						{long && !expanded ? text.slice(0, 1600) + "…" : text}
					</p>
				)
			)}
			<div className="text-muted-foreground flex items-center gap-2 text-xs group-focus-within/message:opacity-100 group-hover/message:opacity-100 [@media(hover:hover)]:opacity-0">
				{long && edit === undefined && (
					<button type="button" onClick={() => setExpanded(!expanded)}>
						{t(expanded ? "tools.less" : "tools.more")}
					</button>
				)}
				{text && <CopyButton text={text} label={t("tools.message")} />}
				{onSend &&
					(edit === undefined ? (
						<button type="button" onClick={() => setEdit(text)}>
							{t("tools.editResend")}
						</button>
					) : (
						<>
							<button type="button" onClick={() => setEdit(undefined)}>
								{t("tools.cancel")}
							</button>
							<Button
								size="sm"
								disabled={busy || (Boolean(text) && !edit.trim())}
								onClick={() => void send(edit).catch(() => {})}
							>
								{t("tools.sendAgain")}
							</Button>
						</>
					))}
			</div>
		</>
	);
}

function ToolCallView({
	item,
	onOpenFile,
}: {
	item: Extract<Item, { type: "toolCall" }>;
	onOpenFile?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation("sessions");
	const look = useAppearance();
	const { detail, status } = item;
	const shown = useShownPath();
	const state =
		status === "inProgress"
			? t("conversation.state.running")
			: status === "failed"
				? t("conversation.state.failed")
				: status === "declined"
					? t("conversation.state.declined")
					: undefined;
	switch (detail.kind) {
		case "command": {
			const exit = detail.exitCode;
			return (
				<Fold
					icon={<SquareTerminal />}
					defaultOpen={look.expandTools}
					title={
						<span className="font-mono text-xs" title={unwrapShell(detail.command).script}>
							{shellSummary(detail.command)}
						</span>
					}
					note={
						exit !== null && exit !== undefined && exit !== 0
							? t("conversation.exitCode", { code: exit })
							: state
					}
					failed={status === "failed" || (exit !== null && exit !== undefined && exit !== 0)}
				>
					<CommandBody detail={detail} duration={item.durationMs} />
				</Fold>
			);
		}
		case "notebookEdit":
			return (
				<Fold
					icon={<FileDiff />}
					title={toolTarget(detail)}
					note={state}
					failed={status === "failed"}
					defaultOpen={look.expandTools}
				>
					<ToolFile path={detail.path} open={onOpenFile} />
					<OpenChange path={detail.path} />
					<NotebookCell detail={detail} />
				</Fold>
			);
		case "fileChange":
			return (
				<Fold
					icon={<FileDiff />}
					defaultOpen={look.expandTools}
					title={
						detail.changes.length === 1
							? t(changeKey(detail.changes[0]?.kind.type), {
									path: shown(detail.changes[0]?.path ?? ""),
								})
							: t("conversation.changedFiles", { count: detail.changes.length })
					}
					note={state}
					failed={status === "failed" || status === "declined"}
				>
					<div className="flex flex-col gap-2">
						{detail.changes.map((change) => (
							<div key={change.path}>
								<p className="text-muted-foreground mb-1 font-mono text-xs" title={change.path}>
									<ToolFile
										path={change.path}
										open={change.kind.type === "delete" ? undefined : onOpenFile}
									/>
									<OpenChange path={change.path} />
								</p>
								{change.kind.type === "add" && !/^@@ |^diff --git |^--- /m.test(change.diff) ? (
									<CreatedCode text={change.diff} path={change.path} open={onOpenFile} />
								) : (
									<Diff
										text={change.diff}
										path={change.path}
										open={change.kind.type === "delete" ? undefined : onOpenFile}
									/>
								)}
							</div>
						))}
					</div>
				</Fold>
			);
		case "mcp":
		case "mcpCall":
			return (
				<Fold
					icon={<Plug />}
					defaultOpen={look.expandTools}
					title={
						<span>
							<span className="text-muted-foreground">{detail.server} · </span>
							<span className="font-mono text-xs">{detail.tool}</span>
						</span>
					}
					note={state ?? (detail.error ? t("conversation.state.failed") : undefined)}
					failed={status === "failed" || Boolean(detail.error)}
				>
					<McpCallBody
						arguments={detail.arguments}
						result={detail.kind === "mcpCall" ? detail.result : undefined}
						error={detail.error}
						running={status === "inProgress"}
					/>
				</Fold>
			);
		case "other":
			return (
				<Fold
					icon={<Wrench />}
					title={toolTarget(detail)}
					note={state}
					failed={status === "failed"}
					defaultOpen={look.expandTools}
				>
					<OtherBody detail={detail} open={onOpenFile} />
				</Fold>
			);
	}
}

/** The text of one file's change: "Created a.rs", "Edited b.rs". */
function changeKey(kind: string | undefined) {
	return kind === "add"
		? "conversation.created"
		: kind === "delete"
			? "conversation.deleted"
			: "conversation.edited";
}

/** One line that opens to show more. */
function Fold({
	icon,
	title,
	note,
	failed,
	dim,
	defaultOpen,
	children,
}: {
	icon: React.ReactNode;
	title: React.ReactNode;
	note?: string;
	failed?: boolean;
	dim?: boolean;
	/** Open at first (Settings → Agent editor: thinking, tool calls). */
	defaultOpen?: boolean;
	children: React.ReactNode;
}) {
	const [open, setOpen] = useState(defaultOpen ?? false);
	return (
		<div>
			<button
				type="button"
				aria-expanded={open}
				onClick={() => setOpen((value) => !value)}
				className={cn(
					"hover:bg-accent/60 focus-visible:ring-ring flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm outline-none focus-visible:ring-2 [&_svg]:size-4 [&_svg]:shrink-0",
					dim ? "text-muted-foreground" : "text-foreground",
				)}
			>
				<ChevronRight
					className={cn(
						"text-muted-foreground transition-transform duration-100",
						open && "rotate-90",
					)}
				/>
				<span className="text-muted-foreground">{icon}</span>
				<span className="min-w-0 flex-1 truncate">{title}</span>
				{note ? (
					<span
						className={cn(
							"shrink-0 text-xs",
							failed ? "text-destructive" : "text-muted-foreground",
						)}
					>
						{note}
					</span>
				) : null}
			</button>
			{open ? <div className="pt-1.5 pb-1 pl-8">{children}</div> : null}
		</div>
	);
}

/** The tool call a request is about, when the conversation has it. */
function requestCall(
	request: Extract<Entry, { kind: "request" }>["request"],
	calls: ReadonlyMap<string, ToolCallItem>,
): ToolCallItem | undefined {
	const id = (request.kind as { toolCallId?: string | null }).toolCallId;
	return id ? calls.get(id) : undefined;
}
