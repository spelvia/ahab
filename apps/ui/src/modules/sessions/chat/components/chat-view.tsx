import { cn } from "cn";
import {
	ArrowDown,
	ArrowLeft,
	Brain,
	ChevronRight,
	Copy,
	GitFork,
	MoreHorizontal,
	ShieldQuestion,
	Sparkles,
} from "lucide-react";
import {
	type ReactNode,
	useCallback,
	useEffect,
	useEffectEvent,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";

import { agentLabel, signInCommand, useAgents } from "@/modules/machines";
import { DropZone, useAttachments } from "@/modules/sessions/attachment";
import {
	useChat,
	useChatModel,
	useChatTurns,
	useChatUsage,
	useNativeModes,
	useAgentHistory,
	useInterruptChat,
	useSetChatModel,
	useModels,
	useRememberChoice,
	useSendMessage,
	readChatCommands,
	useSetMode,
	useSendQueuedNow,
	useUnqueueMessage,
	useReorderQueue,
	useEditQueuedMessage,
} from "@/modules/sessions/chat/api";
import { SubagentStrip, TaskStrip } from "@/modules/sessions/chat/components/activity-strip";
import { PlanMeter, SessionUsageMeter } from "@/modules/sessions/chat/components/chat-meters";
import { ChatFileActions, ChatRoot } from "@/modules/sessions/chat/components/chat-root";
import { Composer } from "@/modules/sessions/chat/components/composer";
import { ConnectionBar } from "@/modules/sessions/chat/components/connection-bar";
import { ConversationView } from "@/modules/sessions/chat/components/conversation";
import { FastToggle } from "@/modules/sessions/chat/components/fast-toggle";
import { ModePicker } from "@/modules/sessions/chat/components/mode-picker";
import { PlanProgress, stepsOf } from "@/modules/sessions/chat/components/plan-progress";
import { QueueList } from "@/modules/sessions/chat/components/queue-list";
import { RequestCard } from "@/modules/sessions/chat/components/request-card";
import { TaskLog } from "@/modules/sessions/chat/components/task-log";
import { toolTarget } from "@/modules/sessions/chat/components/tool-detail";
import { TurnRail } from "@/modules/sessions/chat/components/turn-rail";
import { WorkingLine } from "@/modules/sessions/chat/components/working-line";
import { useSideWorkStops, useWantedMessage } from "@/modules/sessions/chat/hooks";
import {
	agentConversation,
	agentName as nameOfAgent,
	childAgents,
	effortLabel,
	type ObservedAgent,
	observed,
	hasArrived,
	historyUpTo,
	requestSummary,
	transcript,
	railTurns,
	turnStarts,
	workedFor,
} from "@/modules/sessions/chat/model";
import { useConversation } from "@/modules/sessions/chat/stream";
import { draftStore, touchChatTarget } from "@/modules/sessions/reference";
import { problemCode } from "@/shared/api/fetch";
import { type ContextReference, referenceId, toWire } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { copyText } from "@/shared/ui/clipboard";
import { CopyButton } from "@/shared/ui/copy-button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Picker } from "@/shared/ui/picker";
import { toast, toastError } from "@/shared/ui/toast";

/**
 * A chat in a tab: its conversation, live, and the composer. An ended chat
 * keeps its composer: writing to it continues the agent's own session.
 */
export function ChatView({
	chatId,
	sessionId,
	archived,
	root,
	onOpenFile,
	onOpenFileDiff,
	archivedActions,
	onFork,
	onSent,
}: {
	chatId: string;
	/** Its session: context added to it is kept under it. */
	sessionId?: string;
	archived?: boolean;
	/** The chat's project folder: paths inside it show relative to it. */
	root?: string;
	/** Opens a file of the chat's project, at a line. */
	onOpenFileDiff?: (file: string) => Promise<void>;
	onOpenFile?: (path: string, line?: number) => void;
	/** What an archived chat offers instead of its composer (Unarchive, Delete forever). */
	archivedActions?: ReactNode;
	/** Fork from a finished turn: a new chat (in this session, or a new one) holding its history. */
	onFork?: (history: ContextReference, newSession: boolean) => void;
	/** A message was sent from its composer. */
	onSent?: () => void;
}) {
	const { t } = useTranslation("sessions");
	const session = useChat(chatId);
	const conversation = useConversation(chatId);
	const usage = useChatUsage(chatId, conversation.accounting?.turn);
	const wanted = useWantedMessage(chatId);
	const supportsSubmissions = session.data?.messageSubmissions === true;
	const send = useSendMessage(chatId, supportsSubmissions);
	// What was sent shows as "sending" until its echo, matched by submission ID, arrives.
	const outgoingId = send.outgoing?.submissionId;
	const arrived = Boolean(
		outgoingId && hasArrived(conversation.entries, conversation.queue, outgoingId),
	);
	const settleOutgoing = send.settleOutgoing;
	useEffect(() => {
		if (arrived && outgoingId) settleOutgoing(outgoingId);
	}, [arrived, outgoingId, settleOutgoing]);
	const pendingEcho = send.outgoing && !arrived ? send.outgoing : undefined;
	const sendCode = problemCode(send.error);
	const sendUnconfirmed =
		send.error &&
		(send.error.status === 0 ||
			(send.error.status >= 500 && !sendCode) ||
			sendCode === "machine_timeout" ||
			sendCode === "machine_disconnected" ||
			sendCode === "message_unconfirmed");
	const writable =
		!archived && (session.data?.state === "live" || session.data?.resumable === true);
	// Files go to the chat's project, while it can be written to.
	const attachments = useAttachments(writable ? session.data?.projectId : undefined, chatId);
	const interrupt = useInterruptChat();
	const unqueue = useUnqueueMessage();
	const reorderQueue = useReorderQueue();
	const editQueued = useEditQueuedMessage();
	const queueActions = {
		onReorder: (order: string[]) =>
			reorderQueue.mutate({ id: chatId, data: { order } }, { onError: toastError }),
		onEdit: (item: string, text: string) =>
			editQueued.mutateAsync({ id: chatId, item, data: { text } }),
	};
	const sendNow = useSendQueuedNow();
	const setMode = useSetMode();
	const remember = useRememberChoice();
	const end = useRef<HTMLDivElement>(null);
	const data = session.data;
	// Fork: a new chat, with any agent and model, that starts from this
	// conversation's history up to a finished turn (as Paseo does). The
	// history goes as context with its first message.
	const canFork = Boolean(onFork) && conversation.finished?.length;
	const latest = conversation.finished?.at(-1)?.after;
	const entireConversation = async () => {
		if (!conversation.hasOlder) return conversation.entries;
		const older = await conversation.loadOlder(0);
		if (!older) {
			toast(t("chatActions.historyFailed"));
			return undefined;
		}
		return conversation.combineHistory(older);
	};
	const forkFrom = async (after: string, newSession: boolean, done?: () => void) => {
		if (!data) return;
		done?.();
		const history = await entireConversation();
		if (!history) return;
		onFork?.(
			{
				id: referenceId(),
				kind: "chatHistory",
				label: data.title,
				text: historyUpTo(
					history,
					after,
					agentLabel(data.agent),
					t("chatActions.you"),
					t("chatActions.historyCut"),
				),
			},
			newSession,
		);
	};
	// Fork's two choices, in a turn's popover.
	const forkOptions = (after: string, close: () => void) => (
		<div className="flex flex-col gap-0.5">
			<Button
				variant="ghost"
				size="sm"
				className="justify-start"
				disabled={archived}
				onClick={() => forkFrom(after, false, close)}
			>
				{t("chatActions.forkChat")}
			</Button>
			<Button
				variant="ghost"
				size="sm"
				className="justify-start"
				onClick={() => forkFrom(after, true, close)}
			>
				{t("chatActions.forkSession")}
			</Button>
		</div>
	);
	const models = useModels(data?.machineId ?? "", data?.agent);
	// The model and effort it goes on with: chosen for the next message, else its own.
	const isAcp = /^(?:acp|custom):/.test(data?.agent ?? "");
	const nativeModes = useNativeModes(chatId, isAcp && data?.state === "live");
	const chatModel = useChatModel(chatId);
	const setModel = useSetChatModel(chatId);
	const next = chatModel.data?.next ?? undefined;
	const modelId = next?.model ?? chatModel.data?.model ?? data?.model ?? undefined;
	const effortId = next ? next.effort : (chatModel.data?.effort ?? data?.effort);
	const modelList = models.data?.items ?? [];
	const current = modelList.find((item) => item.id === modelId);
	const modelLabel = current?.label ?? modelId;
	const effort = effortId ? effortLabel(effortId) : undefined;
	// Only the chat's own agent's models: the provider never changes (Pi's is its own).
	const switchable = data?.agent !== "pi" && modelList.length > 0;
	const fastNow = next ? next.fast : chatModel.data?.fast;
	const choose = (change: { model?: string; effort?: string | null; fast?: boolean }) =>
		setModel.mutate({
			id: chatId,
			data: {
				model: change.model ?? modelId ?? null,
				effort: change.effort === undefined ? (effortId ?? null) : change.effort,
				fast: change.fast ?? fastNow ?? null,
			},
		});
	// Follows the end of the conversation (see below).
	const following = useRef(true);
	// The subagents shown, from the chat's own agent down:
	// empty shows the chat; each step shows that subagent's conversation.
	const [trail, setTrail] = useState<{ chatId: string; agents: string[] }>({
		chatId,
		agents: [],
	});
	const path = trail.chatId === chatId ? trail.agents : [];
	const goTo = (agents: string[]) => {
		following.current = true;
		setTrail({ chatId, agents });
	};
	const viewing = path.at(-1);
	const history = useAgentHistory(chatId, viewing);
	const subagent = useMemo(
		() =>
			viewing ? agentConversation(history.data?.events, conversation.agents?.[viewing]) : undefined,
		[viewing, history.data, conversation.agents],
	);
	// A subagent's name, or "Subagent" once it is no longer observed.
	const nameOfAgentOr = (agent: ObservedAgent | undefined) =>
		agent ? nameOfAgent(agent) : t("activity.subagent");
	const byId = new Map(observed(conversation.observation).agents.map((agent) => [agent.id, agent]));
	const children = childAgents(conversation.observation, viewing);
	const tasks = observed(conversation.observation).tasks;
	const [taskId, setTaskId] = useState<string>();
	const sideWork = new Map<string, { id: string; type: "agent" | "task"; label: string }[]>();
	for (const agent of children) {
		const id = agent.origin?.itemId;
		if (id)
			sideWork.set(id, [
				...(sideWork.get(id) ?? []),
				{ label: nameOfAgent(agent), id: agent.id, type: "agent" },
			]);
	}
	const statusOf = (id: string) =>
		byId.get(id)?.status ?? tasks.find((task) => task.id === id)?.status;
	const sideStops = useSideWorkStops(chatId, statusOf);
	const shownTask = taskId ? tasks.find((task) => task.id === taskId) : undefined;
	const requests = data?.openRequests ?? [];
	const requestKeys = requests.map((request) => request.id).join("\n");
	// Which requests wait now, by ID; kept while they stay the same, so rows stay memoized.
	const open = useMemo(() => new Set(requestKeys ? requestKeys.split("\n") : []), [requestKeys]);
	// Requests the conversation does not show (not streamed yet): shown after it.
	const unshown = requests.filter(
		(request) =>
			!conversation.entries.some(
				(entry) => entry.kind === "request" && entry.request.id === request.id,
			),
	);
	const live = data?.state === "live";
	const working =
		live && (conversation.working || data?.status === "working" || data?.status === "starting");
	// A message on its way while no turn runs: sent but not yet echoed, or a
	// queued message sent now (its turn stops first).
	const sending =
		live &&
		!working &&
		(send.isPending ||
			Boolean(pendingEcho) ||
			Boolean(conversation.queue?.some((item) => item.interrupt && !item.unconfirmed)));

	// Follow the conversation as it grows (text streaming in, a code block
	// laid out, a request), unless it was scrolled up to read: whatever
	// changes the content's height, while at the bottom, stays at the bottom.
	const entries = subagent?.entries ?? conversation.entries;
	const steps = useMemo(() => {
		for (let i = entries.length - 1; i >= 0; i--) {
			const entry = entries[i];
			if (!entry) continue;
			const plan = stepsOf(entry);
			if (plan) return plan;
			if (entry.kind === "item" && entry.item.type === "userMessage") break;
		}
	}, [entries]);
	// Only the reader scrolling up (a wheel, a swipe, keys, the scroll bar)
	// stops following: content changing under it, or the browser moving
	// it, does not. Back at the bottom, it follows again.
	const reading = useRef(false);
	// Scrolled up: a button brings it back down.
	const [away, setAway] = useState(false);
	const scroller = useRef<HTMLDivElement>(null);
	const toBottom = useCallback((smooth = false) => {
		following.current = true;
		const view = scroller.current;
		if (view)
			view.scrollTo({
				top: view.scrollHeight,
				behavior:
					smooth && !window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
						? "smooth"
						: "auto",
			});
	}, []);
	const sendAgain = send.mutateAsync;
	const historicalPending = useRef(false);
	const resendHistorical = useCallback(
		async (text: string, files: string[]) => {
			if (historicalPending.current) return;
			historicalPending.current = true;
			try {
				toBottom();
				await sendAgain({ text, attachments: files }, { onError: toastError });
				toBottom();
				onSent?.();
			} finally {
				historicalPending.current = false;
			}
		},
		[sendAgain, toBottom, onSent],
	);
	const fileActions = useMemo(
		() => ({ project: data?.projectId, diff: onOpenFileDiff }),
		[data?.projectId, onOpenFileDiff],
	);
	useEffect(() => {
		if (!wanted) return;
		following.current = false;
	}, [wanted]);
	const loadForSearch = useEffectEvent((seq: number) => {
		void conversation.loadOlder(seq);
	});
	const hasOlder = conversation.hasOlder;
	useEffect(() => {
		if (wanted && hasOlder) loadForSearch(wanted.seq);
	}, [wanted, hasOlder]);
	const anchor = useRef<{ row: HTMLElement; top: number }>(null);
	const paging = useRef(false);
	const previousTop = useRef(0);
	const readUp = useRef(false);
	const touchTop = useRef<number | undefined>(undefined);
	const firstEntry = conversation.entries[0]?.key;

	// Followed from when the view is mounted, each time: it can be mounted
	// again as the chat loads, and the new one is followed in its turn.
	const scrollerRef = useCallback((box: HTMLDivElement | null) => {
		scroller.current = box;
		if (!box || typeof ResizeObserver === "undefined") return;
		const stick = () => {
			if (following.current) box.scrollTop = box.scrollHeight;
		};
		const observer = new ResizeObserver(stick);
		// The content growing, and the view itself shrinking (the composer
		// growing, the window).
		if (box.firstElementChild) observer.observe(box.firstElementChild);
		observer.observe(box);
		stick();
		return () => observer.disconnect();
	}, []);
	const newest = conversation.entries.at(-1)?.key;
	useLayoutEffect(() => {
		const view = scroller.current;
		if (newest && view && following.current && !wanted) {
			view.scrollTo({ top: view.scrollHeight, behavior: "instant" });
		}
	}, [newest, wanted]);
	useLayoutEffect(() => {
		const saved = anchor.current;
		anchor.current = null;
		const view = scroller.current;
		if (saved?.row.isConnected && view) {
			view.scrollBy({ top: saved.row.getBoundingClientRect().top - saved.top });
		}
		// An earlier page prepends rows. Leave the reader on the same message.
		// oxlint-disable-next-line react/exhaustive-effect-dependencies
	}, [firstEntry]);
	const loadedTurns = useMemo(() => turnStarts(entries), [entries]);
	const allTurns = useChatTurns(chatId, loadedTurns.at(-1)?.seq);
	const turns = useMemo(() => railTurns(allTurns, loadedTurns), [allTurns, loadedTurns]);
	const agentName = data ? agentLabel(data.agent) : "";
	const answering = useMemo(() => ({ chatId, agent: agentName, open }), [chatId, agentName, open]);
	// A new request is said aloud, and the focus moves to it unless you are typing elsewhere.
	const [announced, setAnnounced] = useState("");
	const previousWorking = useRef(working);
	useEffect(() => {
		if (previousWorking.current !== working && !requests.length)
			setAnnounced(t(working ? "tools.turnStarted" : "tools.turnEnded", { agent: agentName }));
		previousWorking.current = working;
	}, [working, requests.length, agentName, t]);
	const seenRequests = useRef<Set<string> | null>(null);
	const pendingCard = (id?: string) => {
		const cards = [
			...(scroller.current?.querySelectorAll<HTMLElement>("[data-request][data-pending]") ?? []),
		];
		return id ? cards.find((card) => card.dataset.request === id) : cards[0];
	};
	// Runs when the waiting requests (`_keys`) change.
	const onNewRequests = useEffectEvent((_keys: string) => {
		const known = seenRequests.current;
		seenRequests.current = new Set(open);
		// Requests already waiting when the chat opens are shown, not announced.
		if (!known || viewing) return;
		const fresh = requests.find((request) => !known.has(request.id));
		if (!fresh) return;
		setAnnounced(t("chatView.announce", { agent: agentName, summary: requestSummary(fresh) }));
		requestAnimationFrame(() => {
			const card = pendingCard(fresh.id);
			if (!card || card.offsetParent === null) return;
			const active = document.activeElement;
			const typingElsewhere =
				active instanceof HTMLElement &&
				!card.contains(active) &&
				(active.isContentEditable ||
					active instanceof HTMLTextAreaElement ||
					active instanceof HTMLInputElement ||
					active instanceof HTMLSelectElement);
			if (!typingElsewhere) card.focus({ preventScroll: !following.current });
		});
	});
	const loaded = Boolean(data);
	useEffect(() => {
		if (loaded) onNewRequests(requestKeys);
	}, [requestKeys, loaded]);
	const toRequest = () => {
		const card = pendingCard();
		if (!card) return;
		card.scrollIntoView({ block: "center", behavior: "smooth" });
		card.focus({ preventScroll: true });
	};
	const copyConversation = async () => {
		const messages = viewing ? entries : await entireConversation();
		if (!messages) return;
		const done = await copyText(transcript(messages, agentName, t("chatActions.you")));
		toast(done ? t("chatActions.copied") : t("chatActions.failed"));
	};
	const loadOlder = async () => {
		if (paging.current || conversation.loadingOlder || !hasOlder) return;
		paging.current = true;
		following.current = false;
		const view = scroller.current;
		if (view) {
			const edge = view.getBoundingClientRect().top;
			const held = [...view.querySelectorAll<HTMLElement>("[data-entry]")].find(
				(row) => row.getBoundingClientRect().bottom > edge,
			);
			anchor.current = held ? { row: held, top: held.getBoundingClientRect().top } : null;
		}
		const older = await conversation.loadOlder();
		paging.current = false;
		if (!older) {
			anchor.current = null;
			toast(t("chatActions.historyFailed"));
		} else if (!older.entries.length || older.entries[0]?.key === firstEntry) {
			anchor.current = null;
		}
	};

	if (session.isError && !data)
		return (
			<p className="text-muted-foreground p-6 text-sm">
				{t("chatView.cannotLoad")}{" "}
				<Button variant="ghost" size="sm" onClick={() => void session.refetch()}>
					{t("tools.retry")}
				</Button>
			</p>
		);

	return (
		<ChatRoot value={root}>
			<ChatFileActions value={fileActions}>
				<DropZone
					attachments={attachments}
					onReferences={
						writable && sessionId
							? (references) => draftStore.add(sessionId, references)
							: undefined
					}
					className="bg-background flex h-full min-h-0 flex-col"
				>
					<div
						className="relative flex min-h-0 flex-1 flex-col"
						onKeyDown={(event) => {
							if (
								event.defaultPrevented ||
								event.nativeEvent.isComposing ||
								(event.target instanceof Element &&
									event.target.closest(
										"input,textarea,[contenteditable=true],[role=dialog],[data-request]",
									))
							)
								return;
							if (
								event.key === "Escape" &&
								working &&
								!viewing &&
								!open.size &&
								!interrupt.isPending
							) {
								event.preventDefault();
								interrupt.mutate({ id: chatId }, { onError: toastError });
							} else if (
								event.key === "i" &&
								!event.metaKey &&
								!event.ctrlKey &&
								!event.altKey &&
								writable
							) {
								const field = event.currentTarget.parentElement?.querySelector<HTMLElement>(
									"[data-chat-composer] textarea,[data-chat-composer] [contenteditable=true]",
								);
								if (field) {
									event.preventDefault();
									field.focus();
								}
							}
						}}
					>
						<ConnectionBar
							link={conversation.link}
							retryAt={conversation.retryAt}
							reconnect={conversation.reconnect}
						/>
						{viewing ? (
							<nav
								aria-label={t("subagent.breadcrumb")}
								className="bg-background/90 border-border flex h-10 shrink-0 items-center gap-1 border-b px-3 text-xs backdrop-blur"
							>
								<button
									type="button"
									aria-label={t("subagent.back", {
										name:
											path.length > 1
												? nameOfAgentOr(byId.get(path.at(-2) ?? ""))
												: t("subagent.chat"),
									})}
									onClick={() => goTo(path.slice(0, -1))}
									className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-7 shrink-0 place-items-center rounded-md"
								>
									<ArrowLeft className="size-4" />
								</button>
								<button
									type="button"
									onClick={() => goTo([])}
									className="text-muted-foreground hover:text-foreground shrink-0 rounded px-1 py-0.5"
								>
									{t("subagent.chat")}
								</button>
								{path.map((id, index) => (
									<span key={id} className="flex min-w-0 items-center gap-1">
										<ChevronRight className="text-muted-foreground size-3.5 shrink-0" aria-hidden />
										{index === path.length - 1 ? (
											<span className="text-heading truncate font-medium" aria-current="page">
												{nameOfAgentOr(byId.get(id))}
											</span>
										) : (
											<button
												type="button"
												onClick={() => goTo(path.slice(0, index + 1))}
												className="text-muted-foreground hover:text-foreground truncate rounded px-1 py-0.5"
											>
												{nameOfAgentOr(byId.get(id))}
											</button>
										)}
									</span>
								))}
							</nav>
						) : null}
						{viewing ? null : <TurnRail chatId={chatId} turns={turns} scroller={scroller} />}
						<div className="flex shrink-0 justify-end px-3 py-1">
							<DropdownMenu>
								<DropdownMenuTrigger
									aria-label={t("chatActions.label")}
									className="text-muted-foreground hover:text-foreground hover:bg-accent bg-background/80 focus-visible:ring-ring grid size-7 place-items-center rounded-md backdrop-blur outline-none focus-visible:ring-2"
								>
									<MoreHorizontal className="size-4" />
								</DropdownMenuTrigger>
								<DropdownMenuContent align="end">
									{canFork && latest != null && (
										<DropdownMenuSub>
											<DropdownMenuSubTrigger>
												<GitFork />
												{t("turnFooter.fork")}
											</DropdownMenuSubTrigger>
											<DropdownMenuSubContent>
												<DropdownMenuItem
													disabled={archived}
													onSelect={() => forkFrom(latest, false)}
												>
													{t("chatActions.forkChat")}
												</DropdownMenuItem>
												<DropdownMenuItem onSelect={() => forkFrom(latest, true)}>
													{t("chatActions.forkSession")}
												</DropdownMenuItem>
											</DropdownMenuSubContent>
										</DropdownMenuSub>
									)}
									<DropdownMenuItem disabled={entries.length === 0} onSelect={copyConversation}>
										<Copy />
										{t("chatActions.copy")}
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</div>
						<div
							ref={scrollerRef}
							data-chat-scroll
							tabIndex={0}
							className="min-h-0 flex-1 overflow-auto"
							onWheel={(event) => {
								if (event.deltaY < 0) {
									following.current = false;
									readUp.current = true;
									if (!viewing && event.currentTarget.scrollTop <= 160) {
										readUp.current = false;
										void loadOlder();
									}
								}
							}}
							onTouchStart={(event) => {
								touchTop.current = event.touches[0]?.clientY;
							}}
							onTouchMove={(event) => {
								reading.current = true;
								const y = event.touches[0]?.clientY;
								if (
									!viewing &&
									y !== undefined &&
									touchTop.current !== undefined &&
									y - touchTop.current > 24 &&
									event.currentTarget.scrollTop <= 160
								) {
									touchTop.current = undefined;
									void loadOlder();
								}
							}}
							onTouchEnd={() => {
								reading.current = false;
								touchTop.current = undefined;
							}}
							onPointerDown={(event) => {
								// On the scroll bar (the scroller itself, not its content).
								if (event.target === event.currentTarget) reading.current = true;
							}}
							onPointerUp={() => {
								reading.current = false;
							}}
							onKeyDown={(event) => {
								if (["ArrowUp", "PageUp", "Home"].includes(event.key)) {
									following.current = false;
									readUp.current = true;
									if (!viewing && event.currentTarget.scrollTop <= 160) {
										readUp.current = false;
										void loadOlder();
									}
								}
							}}
							onScroll={(event) => {
								const view = event.currentTarget;
								const movedUp = view.scrollTop < previousTop.current;
								previousTop.current = view.scrollTop;
								if (
									!viewing &&
									movedUp &&
									view.scrollTop <= 160 &&
									(readUp.current || reading.current)
								) {
									readUp.current = false;
									void loadOlder();
								}
								const below = view.scrollHeight - view.scrollTop - view.clientHeight;
								if (below < 80 && !paging.current) following.current = true;
								else if (reading.current || readUp.current) following.current = false;
								setAway(below > 200);
							}}
						>
							<div
								className={cn(
									"mx-auto flex max-w-3xl flex-col gap-5 px-4 pb-10 sm:px-6 md:px-10",
									viewing ? "pt-4" : "pt-1",
								)}
							>
								{subagent ? (
									<>
										<ConversationView
											sideWork={sideWork}
											onOpenRelated={(type, id) =>
												type === "agent" ? goTo([...path, id]) : setTaskId(id)
											}
											key={viewing}
											entries={subagent.entries}
											projectId={data?.projectId}
											onOpenFile={onOpenFile}
										/>
										{history.isPending && subagent.entries.length === 0 ? (
											<p className="text-muted-foreground text-sm">{t("subagent.loading")}</p>
										) : history.isError && subagent.entries.length === 0 ? (
											<p className="text-muted-foreground text-sm">{t("subagent.unsupported")}</p>
										) : !subagent.working && subagent.entries.length === 0 ? (
											<p className="text-muted-foreground text-sm">{t("subagent.empty")}</p>
										) : null}
										{subagent.working ? (
											<WorkingLine
												entries={subagent.entries}
												agent={nameOfAgentOr(byId.get(viewing ?? ""))}
												since={subagent.turnStartedAt}
											/>
										) : null}
									</>
								) : (
									<>
										{conversation.hasOlder ? (
											<Button
												variant="ghost"
												size="sm"
												className="self-center"
												disabled={conversation.loadingOlder}
												onClick={() => void loadOlder()}
											>
												{t(
													conversation.loadingOlder
														? "chatActions.loadingHistory"
														: "chatActions.loadHistory",
												)}
											</Button>
										) : null}
										{conversation.link === "open" &&
											data &&
											!working &&
											!conversation.entries.length &&
											!pendingEcho && (
												<p className="text-muted-foreground text-sm">{t("tools.empty")}</p>
											)}
										<ConversationView
											sideWork={sideWork}
											onOpenRelated={(type, id) =>
												type === "agent" ? goTo([...path, id]) : setTaskId(id)
											}
											onResend={
												writable &&
												!working &&
												!send.isPending &&
												!send.recovered &&
												!sendUnconfirmed
													? resendHistorical
													: undefined
											}
											entries={conversation.entries}
											projectId={data?.projectId}
											onOpenFile={onOpenFile}
											chatId={chatId}
											finished={conversation.finished}
											forkOptions={canFork ? forkOptions : undefined}
											onAddContext={
												writable && sessionId
													? (references) => draftStore.add(sessionId, references)
													: undefined
											}
											requests={answering}
										/>
										{data &&
											(send.isError ||
												conversation.entries.at(-1)?.kind === "turnEnd" ||
												conversation.entries.at(-1)?.kind === "error") && (
												<SignIn machine={data.machineId} agent={data.agent} />
											)}
										{pendingEcho && (
											<p
												role="status"
												className="bg-muted ml-auto max-w-[90%] rounded-xl px-3 py-2 text-sm whitespace-pre-wrap"
											>
												{pendingEcho.text}{" "}
												<span className="text-muted-foreground">{t("composer.sending")}</span>
											</p>
										)}
										{unshown.map((request) => (
											<RequestCard
												key={request.id}
												chatId={chatId}
												request={request}
												pending
												agent={agentName}
											/>
										))}
										{working && requests.length === 0 ? (
											<WorkingLine
												entries={conversation.entries}
												agent={agentName}
												since={conversation.turnStartedAt}
											/>
										) : sending ? (
											// Sent, and the agent's turn not started yet: still a spinner.
											<WorkingLine
												agent={agentName}
												label={t("workingLine.sending", { agent: agentName })}
											/>
										) : null}
									</>
								)}
								<div ref={end} />
							</div>
						</div>
						<p aria-live="polite" className="sr-only">
							{announced}
						</p>
						{away && open.size > 0 && !viewing ? (
							<button
								type="button"
								title={t("chatView.needsAnswerTitle")}
								onClick={toRequest}
								className="bg-popover border-warning/60 text-warning hover:bg-accent focus-visible:ring-ring mx-auto my-1 flex h-11 shrink-0 items-center gap-2 rounded-full border pr-3.5 pl-3 text-sm font-medium whitespace-nowrap shadow-lg outline-none focus-visible:ring-2 sm:h-9"
							>
								<ShieldQuestion className="size-4" aria-hidden />
								{t("chatView.needsAnswer")}
								<ArrowDown className="size-4" aria-hidden />
							</button>
						) : away ? (
							<button
								type="button"
								aria-label={t("chatView.toBottom")}
								title={t("chatView.toBottom")}
								onClick={() => toBottom(true)}
								className={cn(
									"bg-popover border-border text-foreground hover:bg-accent focus-visible:ring-ring mx-auto my-1 flex h-9 shrink-0 items-center gap-2 rounded-full border shadow-lg outline-none focus-visible:ring-2",
									working && !viewing ? "border-primary/60 pr-3 pl-3.5" : "w-9 justify-center",
								)}
							>
								{working && !viewing ? (
									<WorkingSummary agent={agentName} since={conversation.turnStartedAt} />
								) : null}
								<ArrowDown className="size-4" />
							</button>
						) : null}
					</div>
					<div className="mx-auto w-full max-w-3xl shrink-0 px-2 pt-1.5 pb-2 sm:px-6 sm:pt-2 sm:pb-5 md:px-10">
						{working && steps?.length ? <PlanProgress steps={steps} pinned /> : null}
						{children.length > 0 || (!viewing && tasks.length > 0) ? (
							<div className="mb-2 flex flex-col gap-1.5">
								<SubagentStrip
									activity={(id) => {
										const last = conversation.agents?.[id]?.findLast((event) =>
											["itemEnd", "itemStart", "turnEnd"].includes(event.event.type),
										);
										const item = last?.event.item as
											| import("@/modules/sessions/chat/model").Item
											| undefined;
										return item?.type === "toolCall" ? toolTarget(item.detail) : undefined;
									}}
									stops={sideStops.stops}
									onStop={
										live && conversation.observation?.canStopAgent
											? (id, name) => sideStops.stop("agent", id, name)
											: undefined
									}
									agents={children}
									onOpen={(agent) => goTo([...path, agent.id])}
								/>
								{viewing ? null : (
									<TaskStrip
										stops={sideStops.stops}
										onStop={
											live && conversation.observation?.canStopTask
												? (id, name) => sideStops.stop("task", id, name)
												: undefined
										}
										tasks={tasks}
										onOpen={(task) => setTaskId(task.id)}
									/>
								)}
							</div>
						) : null}
						<TaskLog chatId={chatId} task={shownTask} onClose={() => setTaskId(undefined)} />
						{!viewing && writable && send.recovered ? (
							<div className="border-border bg-card mb-2 flex flex-wrap items-center gap-2 rounded-lg border p-2 text-sm">
								<div className="min-w-0 flex-1 px-1">
									<p>{t("chatView.recoveredSubmission")}</p>
									{send.recovered.text ? (
										<p className="text-muted-foreground truncate" title={send.recovered.text}>
											{send.recovered.text}
										</p>
									) : null}
								</div>
								<Button
									size="sm"
									variant="outline"
									disabled={send.isPending}
									onClick={() => {
										if (send.recovered)
											send.mutate(send.recovered, {
												onSuccess: () => {
													toBottom();
													onSent?.();
												},
											});
									}}
								>
									{t("chatView.retrySubmission")}
								</Button>
								<Button
									size="sm"
									variant="ghost"
									disabled={send.isPending}
									onClick={send.dismissSubmission}
								>
									{t("chatView.dismissSubmission")}
								</Button>
							</div>
						) : null}
						{!viewing && !writable && conversation.queue?.length ? (
							<div className="mb-2">
								<QueueList
									queue={conversation.queue}
									onUnqueue={(item) =>
										unqueue.mutate({ id: chatId, item }, { onError: toastError })
									}
									{...queueActions}
								/>
							</div>
						) : null}
						{viewing ? (
							<p className="text-muted-foreground border-border rounded-lg border border-dashed px-3 py-2 text-xs">
								{t("subagent.readOnly")}
							</p>
						) : writable ? (
							<Composer
								projectId={data?.projectId}
								commandChat={live ? chatId : undefined}
								readCommands={readChatCommands}
								attachments={attachments}
								placeholder={
									!live
										? t("chatView.placeholder.ended")
										: working
											? t("chatView.placeholder.working")
											: t("chatView.placeholder.idle")
								}
								working={working}
								pending={send.isPending}
								draftKey={sessionId}
								messageDraftKey={chatId}
								onOpenFile={onOpenFile}
								onType={() => {
									if (sessionId && data) touchChatTarget(data.projectId, sessionId);
								}}
								queue={conversation.queue}
								onUnqueue={(item) => unqueue.mutate({ id: chatId, item }, { onError: toastError })}
								onSendNow={(item) => sendNow.mutate({ id: chatId, item }, { onError: toastError })}
								onReorder={queueActions.onReorder}
								onEditQueued={queueActions.onEdit}
								onSend={async (text, attached, references, delivery, command) => {
									if (text || command) toBottom();
									await send.mutateAsync({
										text,
										attachments: attached,
										references: references.map(toWire),
										command,
										delivery,
									});
									// What you sent shows at the end, and the chat follows it.
									toBottom();
									onSent?.();
									if (sessionId && references.length)
										draftStore.remove(
											sessionId,
											references.map((item) => item.id),
										);
								}}
								stopping={interrupt.isPending}
								awaitingAnswer={open.size > 0}
								settingsSummary={[modelLabel, effort].filter(Boolean).join(" · ")}
								settings={
									<>
										{switchable ? (
											<Picker
												label={t("startForm.model")}
												icon={<Sparkles />}
												value={modelId}
												disabled={setModel.isPending}
												options={modelList.map((item) => ({
													value: item.id,
													label: item.label,
													description: item.description || undefined,
												}))}
												onChange={(id) => {
													const picked = modelList.find((item) => item.id === id);
													const keeps = picked?.efforts.some((item) => item.id === effortId);
													choose({
														model: id,
														effort: keeps ? effortId : (picked?.defaultEffort ?? null),
													});
												}}
											/>
										) : modelLabel ? (
											<span
												className="text-muted-foreground flex items-center gap-1 px-1 text-xs"
												title={t("chatView.modelTitle")}
											>
												<Sparkles className="size-3.5" />
												{modelLabel}
											</span>
										) : null}
										{switchable && current && current.efforts.length > 0 ? (
											<Picker
												label={t("startForm.reasoning")}
												icon={<Brain />}
												value={effortId ?? ""}
												disabled={setModel.isPending}
												options={current.efforts.map((item) => ({
													value: item.id,
													label: effortLabel(item.id),
													description: item.description || undefined,
												}))}
												onChange={(id) => choose({ effort: id || null })}
											/>
										) : effort ? (
											<span
												className="text-muted-foreground flex items-center gap-1 px-1 text-xs"
												title={t("chatView.effortTitle")}
											>
												<Brain className="size-3.5" />
												{effort}
											</span>
										) : null}
										{next ? (
											<span
												className="text-primary border-primary/40 rounded-md border px-1.5 py-0.5 text-xs"
												title={t("modelSwitch.nextTitle", { model: modelLabel ?? "" })}
											>
												{t("modelSwitch.next")}
											</span>
										) : null}
										<ModePicker
											agent={data?.agent}
											native={isAcp ? nativeModes.data : undefined}
											value={isAcp ? nativeModes.data?.current : (data?.mode ?? undefined)}
											disabled={setMode.isPending}
											onChange={(mode) => {
												setMode.mutate(
													{ id: chatId, data: { mode } },
													{
														onSuccess: () => {
															if (isAcp) void nativeModes.refetch();
															else if (data) remember(data.agent, { mode });
														},
														onError: toastError,
													},
												);
												// The next chat with this agent starts in it too.
											}}
										/>
										{data?.unrestricted ? (
											<span
												className="text-warning border-warning/40 rounded-md border px-1.5 py-0.5 text-xs"
												title={t("chatView.unrestrictedTitle")}
											>
												{t("chatView.unrestricted")}
											</span>
										) : null}
										{switchable ? (
											<FastToggle
												fast={current?.fast}
												value={fastNow}
												disabled={setModel.isPending}
												onChange={(fast) => choose({ fast })}
											/>
										) : null}
									</>
								}
								meters={
									<>
										{data ? <PlanMeter machineId={data.machineId} agent={data.agent} /> : null}
										<SessionUsageMeter
											context={conversation.context}
											compacting={conversation.compacting !== undefined}
											spend={usage}
										/>
									</>
								}
								onStop={() => interrupt.mutate({ id: chatId }, { onError: toastError })}
							/>
						) : archived ? (
							<div className="border-border bg-card/60 flex flex-wrap items-center justify-center gap-3 rounded-xl border px-4 py-3">
								<p className="text-muted-foreground text-sm">{t("chatView.archived")}</p>
								{archivedActions}
							</div>
						) : canFork && latest ? (
							// Its agent's session was not kept, so it cannot be resumed:
							// it goes on in a new chat that carries its history.
							<div className="border-border bg-card/60 flex flex-wrap items-center justify-center gap-3 rounded-xl border px-4 py-3">
								<p className="text-muted-foreground text-sm">{t("chatView.notResumable")}</p>
								<Button size="sm" onClick={() => forkFrom(latest, false)}>
									{t("chatView.continueInNewChat")}
								</Button>
							</div>
						) : (
							<p className="text-muted-foreground text-center text-sm">{t("chatView.ended")}</p>
						)}
						{send.isError || setMode.isError || setModel.isError ? (
							<div className="mt-2 space-y-1">
								<p role="alert" className="text-destructive text-sm">
									{sendUnconfirmed
										? t(
												supportsSubmissions
													? "chatView.sendUnconfirmed"
													: "chatView.sendUnconfirmedLegacy",
											)
										: (send.error?.message ?? setMode.error?.message ?? setModel.error?.message)}
								</p>
								{sendCode === "message_unconfirmed" && !send.recovered ? (
									<Button size="sm" variant="outline" onClick={send.dismissSubmission}>
										{t("chatView.startNewSubmission")}
									</Button>
								) : null}
							</div>
						) : null}
					</div>
				</DropZone>
			</ChatFileActions>
		</ChatRoot>
	);
}

/** In the button back down, while the agent works: who, and for how long. */
function WorkingSummary({ agent, since }: { agent: string; since?: number }) {
	const { t } = useTranslation("sessions");
	const [mounted] = useState(() => Date.now());
	const [now, setNow] = useState(mounted);
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);
	return (
		<span className="flex items-center gap-2 text-xs tabular-nums">
			<span className="relative flex size-2" aria-hidden>
				<span className="bg-primary absolute inline-flex size-full rounded-full opacity-60 motion-safe:animate-ping" />
				<span className="bg-primary relative inline-flex size-2 rounded-full" />
			</span>
			{t("workingLine.scrolledWorking", {
				agent,
				time: workedFor(Math.max(0, now - (since ?? mounted))),
			})}
		</span>
	);
}

function SignIn({ machine, agent }: { machine: string; agent: string }) {
	const { t } = useTranslation("sessions");
	const status = useAgents(machine).data?.items.find(
		(item) => item.agent === agent && item.state === "notSignedIn",
	);
	if (!status) return null;
	const command = signInCommand(agent);
	return (
		<section
			aria-label={t("tools.signIn", { agent: status.label })}
			className="border-border space-y-1 rounded-md border p-3 text-sm"
		>
			<h3>{t("tools.signIn", { agent: status.label })}</h3>
			{status.detail && <p>{status.detail}</p>}
			{command && (
				<div className="flex items-center gap-1">
					<code>{command}</code>
					<CopyButton text={command} label={t("tools.command")} />
				</div>
			)}
			<a
				className="text-xs underline"
				href={"/settings/machines/" + encodeURIComponent(machine) + "/agents"}
			>
				{t("tools.machineAgents")}
			</a>
		</section>
	);
}
