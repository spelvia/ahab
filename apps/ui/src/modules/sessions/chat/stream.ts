import { useEffect, useEffectEvent, useMemo, useReducer, useRef, useState } from "react";

import { useChatRefresh, useReadChatHistory } from "@/modules/sessions/chat/api";
import { apply, type Conversation, empty, type StreamEvent } from "@/modules/sessions/chat/model";
import { onResume, retryDelay } from "@/shared/api/resume";

/** Events after which the server's view of the chat (its state, requests) changed. */
const CHANGES_CHAT = new Set([
	"turnStart",
	"turnEnd",
	"status",
	"request",
	"requestResolved",
	"closed",
]);

type History = Pick<Conversation, "entries" | "finished">;
type LoadOlder = (through?: number) => Promise<History | undefined>;

function addHistory(older: History, newer: History): History {
	// A long turn can span pages. Preserve the item's first position and
	// use its latest version when both pages contain it.
	const entries = new Map(older.entries.map((entry) => [entry.key, entry]));
	for (const entry of newer.entries) entries.set(entry.key, entry);
	return {
		entries: [...entries.values()],
		finished: [...(older.finished ?? []), ...(newer.finished ?? [])],
	};
}

type Action =
	| { type: "events"; events: StreamEvent[] }
	| { type: "seed"; conversation: Conversation }
	| { type: "reset" };

function reduce(conversation: Conversation, action: Action): Conversation {
	if (action.type === "seed") return action.conversation;
	return action.type === "reset" ? empty : action.events.reduce(apply, conversation);
}

/** The stream's connection: first opening, open, or lost and trying again. */
export type Link = "connecting" | "open" | "reconnecting";

/**
 * A chat's conversation, live: its kept events, then new ones as they
 * happen, text included as it is typed. Reconnects after a dropped
 * connection, from the last event it has.
 */
export function useConversation(chatId: string) {
	const [conversation, dispatch] = useReducer(reduce, empty);
	// Kept with its chat: another chat starts out connecting.
	const [linked, setLinked] = useState<{ chatId: string; link: Link }>();
	const link = linked?.chatId === chatId ? linked.link : "connecting";
	const refresh = useChatRefresh();
	const changed = useEffectEvent(() => refresh(chatId));
	const readHistory = useReadChatHistory();
	const retry = useRef<() => void>(undefined);
	const loader = useRef<LoadOlder>(undefined);
	const [history, setHistory] = useState<{ chatId: string; page: History; before: number }>();
	const [loadingChat, setLoadingChat] = useState<string>();
	const previous = history?.chatId === chatId ? history : undefined;
	const shown = useMemo(
		() => (previous?.page.entries.length ? addHistory(previous.page, conversation) : conversation),
		[previous, conversation],
	);

	useEffect(() => {
		dispatch({ type: "reset" });
		const abort = new AbortController();
		const historyState: { before: number; page: History; loading?: Promise<History | undefined> } =
			{
				before: 0,
				page: empty,
			};
		const setLink = (next: Link) => setLinked({ chatId, link: next });
		const connection = {
			last: 0,
			source: undefined as EventSource | undefined,
			retry: undefined as ReturnType<typeof setTimeout> | undefined,
			failures: 0,
			wasOpen: false,
			ready: false,
			opening: false,
			queued: [] as StreamEvent[],
			frame: undefined as number | undefined,
			background: undefined as ReturnType<typeof setTimeout> | undefined,
		};
		// Apply once per frame, with a count and time bound for hidden windows.
		const flush = () => {
			if (connection.frame !== undefined) cancelAnimationFrame(connection.frame);
			clearTimeout(connection.background);
			connection.frame = undefined;
			connection.background = undefined;
			const events = connection.queued;
			connection.queued = [];
			if (!events.length) return;
			dispatch({ type: "events", events });
			// The chat is asked for again as soon as its state changes, not
			// only on its slow poll.
			if (events.some((item) => item.seq !== null && CHANGES_CHAT.has(item.event.type))) changed();
		};
		const connect = () => {
			if (abort.signal.aborted || typeof EventSource === "undefined") return;
			const opened = new EventSource(`/api/v1/chats/${chatId}/stream?after=${connection.last}`);
			connection.source = opened;
			opened.addEventListener("open", () => {
				setLink("open");
				if (connection.wasOpen) changed();
				connection.wasOpen = true;
			});
			opened.addEventListener("message", (message) => {
				if (connection.source !== opened || abort.signal.aborted) return;
				connection.failures = 0;
				const event = JSON.parse(String(message.data)) as StreamEvent;
				if (event.seq !== null) connection.last = Math.max(connection.last, event.seq);
				connection.queued.push(event);
				if (connection.queued.length >= 256) flush();
				else {
					connection.frame ??= requestAnimationFrame(flush);
					connection.background ??= setTimeout(flush, 100);
				}
			});
			opened.addEventListener("error", () => {
				if (connection.source !== opened || abort.signal.aborted) return;
				opened.close();
				connection.source = undefined;
				setLink("reconnecting");
				connection.retry = setTimeout(connect, retryDelay(connection.failures++));
			});
		};
		const fetchPage = async (cursor: number) => {
			const result = await readHistory(chatId, cursor, abort.signal);
			if (typeof result.historyBefore !== "number") return undefined;
			return {
				before: result.historyBefore,
				conversation: result.items.reduce(
					(acc, item) =>
						apply(acc, {
							seq: item.seq,
							event: item.event as StreamEvent["event"],
							at: item.createdAt,
						}),
					empty,
				),
			};
		};
		const publishHistory = () =>
			setHistory({ chatId, page: historyState.page, before: historyState.before });
		loader.current = async (through) => {
			if (historyState.loading) {
				await historyState.loading;
				if (through === undefined || !historyState.before || historyState.before <= through) {
					return historyState.page;
				}
			}
			if (!historyState.before || abort.signal.aborted) return historyState.page;
			setLoadingChat(chatId);
			historyState.loading = (async () => {
				try {
					for (;;) {
						// A page supplies the cursor for the next one.
						// oxlint-disable-next-line no-await-in-loop
						const result = await fetchPage(historyState.before);
						if (abort.signal.aborted || !result || result.before >= historyState.before)
							return undefined;
						historyState.before = result.before;
						historyState.page = addHistory(result.conversation, historyState.page);
						if (!result.before || through === undefined || result.before <= through)
							return historyState.page;
					}
				} catch {
					return undefined;
				} finally {
					historyState.loading = undefined;
					if (!abort.signal.aborted) {
						publishHistory();
						setLoadingChat(undefined);
					}
				}
			})();
			return historyState.loading;
		};
		const openRecent = () => {
			if (connection.opening || abort.signal.aborted) return;
			connection.opening = true;
			void fetchPage(0)
				.then((result) => {
					connection.opening = false;
					if (abort.signal.aborted) return;
					if (result) {
						historyState.before = result.before;
						connection.last = result.conversation.last;
						publishHistory();
						dispatch({ type: "seed", conversation: result.conversation });
					}
					// A server that predates recent pages still has its legacy replay.
					connection.ready = true;
					connection.failures = 0;
					setLoadingChat(undefined);
					connect();
				})
				.catch(() => {
					if (abort.signal.aborted) return;
					connection.opening = false;
					// A transient API failure must not turn opening into an all-history replay.
					setLink("reconnecting");
					connection.retry = setTimeout(openRecent, retryDelay(connection.failures++));
				});
		};
		openRecent();
		// Reconnects now: asked to, or back on the page. A stale stream may
		// look open but be dead, so it is opened again from the last event.
		const reconnect = (stale = false) => {
			if (connection.source && !stale) return;
			clearTimeout(connection.retry);
			connection.failures = 0;
			if (connection.source) {
				connection.source.close();
				connection.source = undefined;
			}
			if (connection.ready) connect();
			else openRecent();
		};
		retry.current = () => reconnect();
		const stopResume = onResume(reconnect);
		return () => {
			abort.abort();
			loader.current = undefined;
			retry.current = undefined;
			stopResume();
			clearTimeout(connection.retry);
			clearTimeout(connection.background);
			if (connection.frame !== undefined) cancelAnimationFrame(connection.frame);
			connection.source?.close();
		};
	}, [chatId, readHistory]);

	return {
		...conversation,
		entries: shown.entries,
		finished: shown.finished,
		link,
		reconnect: () => retry.current?.(),
		hasOlder: Boolean(previous?.before),
		loadingOlder: loadingChat === chatId,
		loadOlder: (through?: number) => loader.current?.(through),
		combineHistory: (older: History) => addHistory(older, conversation).entries,
	};
}
