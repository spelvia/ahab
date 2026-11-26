import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

import type { StreamEvent } from "@/modules/sessions/chat/model";
import { ApiError } from "@/shared/api/fetch";
import type {
	AgentDefaults,
	AgentDefaultsList,
	SendMessage,
} from "@/shared/api/generated/ahab.schemas";
import {
	getGetChatQueryKey,
	getChat,
	getListAgentDefaultsQueryKey,
	useAnswerRequest as useGeneratedAnswerRequest,
	getTaskOutput,
	useGetChatModes,
	useStopChatWork,
	listChatCommands,
	listProjectCommands,
	useGetAgentHistory,
	useGetChat,
	useInterruptChat as useGeneratedInterruptChat,
	useListAgentDefaults,
	useListEditedFiles,
	getListModelsQueryKey,
	listModels,
	listEvents,
	useListModels,
	useSaveAgentDefaults,
	sendMessage,
	useCloseChat as useGeneratedCloseChat,
	useRemoveChat as useGeneratedRemoveChat,
	useGetChatModel,
	useGetChatTurns,
	useGetChatUsage,
	useSearchSessions,
	useSetChatModel as useGeneratedSetChatModel,
	useSetChatMode,
	useStartChat as useGeneratedStartChat,
	useUpdateChat as useGeneratedUpdateChat,
	useSendQueuedNow as useGeneratedSendQueuedNow,
	useUnqueueMessage as useGeneratedUnqueueMessage,
	useReorderQueue as useGeneratedReorderQueue,
	useEditQueuedMessage as useGeneratedEditQueuedMessage,
} from "@/shared/api/generated/sessions-chat/sessions-chat";
import { useInvalidate } from "@/shared/api/invalidate";
import { t } from "@/shared/i18n";
import {
	type PendingMessage,
	pendingChatMessage,
	rememberPendingChatMessage,
} from "@/shared/preferences/store";

/** Reads a page without retaining a second copy of the event payload in the query cache. */
export function useReadChatHistory() {
	const client = useQueryClient();
	return useCallback(
		(id: string, before: number, signal: AbortSignal) =>
			client.fetchQuery({
				queryKey: ["chatHistory", id, before],
				queryFn: () => listEvents(id, { before, bounded: true }, { signal, timeoutMs: 12_000 }),
				gcTime: 0,
				retry: false,
			}),
		[client],
	);
}

/**
 * One chat: its state and open requests. Its live stream asks for it again
 * whenever they change (useConversation); the slow poll is a safety net.
 */
export function useChat(id: string) {
	return useGetChat(id, { query: { refetchInterval: 15_000 } });
}

/**
 * A subagent's conversation so far (none asked: nothing). Read once: what it
 * does next comes live on the chat's stream.
 */
export function useAgentHistory(chatId: string, agentId: string | undefined) {
	return useGetAgentHistory(chatId, agentId ?? "", {
		query: {
			enabled: Boolean(agentId),
			staleTime: Number.POSITIVE_INFINITY,
			retry: false,
			select: (history) => ({
				events: history.events as unknown as StreamEvent["event"][],
			}),
		},
	});
}

/** A page of a background task's output, from `cursor` (none: its start). */
export function readTaskOutput(chatId: string, taskId: string, cursor: string | undefined) {
	return getTaskOutput(chatId, taskId, { cursor, limit: 65_536 });
}

/** The files a session's agents edited, as they named them (usually absolute). */
export function useEditedFiles(sessionId: string | undefined) {
	return useListEditedFiles(sessionId ?? "", {
		query: { enabled: Boolean(sessionId), refetchInterval: 10_000, retry: false },
	});
}

/** Asks for a chat again (its stream saw its state change). */
export function useChatRefresh() {
	const queryClient = useQueryClient();
	return useCallback(
		(id: string) => void queryClient.invalidateQueries({ queryKey: getGetChatQueryKey(id) }),
		[queryClient],
	);
}

/**
 * The models an agent offers on a machine, with their reasoning efforts:
 * the list Ahab keeps (it reads it again in the background when old).
 */
export function useModels(machineId: string, agent: string | undefined) {
	return useListModels(machineId, agent ?? "", undefined, {
		query: {
			enabled: Boolean(machineId && agent),
			staleTime: 5 * 60_000,
			retry: false,
			// While Ahab reads them again, ask a little later for the new list.
			refetchInterval: (query) => (query.state.data?.refreshing ? 15_000 : false),
		},
	});
}

/** Asks the agent for its models now, and shows the answer. */
export function useRefreshModels(machineId: string, agent: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: () => listModels(machineId, agent, { refresh: true }),
		onSuccess: (list) => queryClient.setQueryData(getListModelsQueryKey(machineId, agent), list),
	});
}

/** Refreshes everything about sessions and chats after a change. */
function useRefresh() {
	// Sessions, their chats and folders, and the waiting counts.
	return useInvalidate("/sessions", "/chats", "/session-folders");
}

/** Each agent's defaults: the model, effort and mode a new chat starts with. */
export function useAgentDefaults() {
	return useListAgentDefaults({ query: { staleTime: 60_000 } });
}

/** What can change in an agent's defaults. */
export type DefaultsChange = Partial<Pick<AgentDefaults, "model" | "effort" | "mode">>;

/**
 * Saves a choice as the agent's default, on every device. The list updates
 * at once, so the pickers follow without waiting for the server.
 */
export function useRememberChoice() {
	const queryClient = useQueryClient();
	const save = useSaveAgentDefaults();
	return (agent: string, change: DefaultsChange) => {
		const key = getListAgentDefaultsQueryKey();
		const list = queryClient.getQueryData<AgentDefaultsList>(key);
		const current = list?.items.find((item) => item.agent === agent);
		const next = {
			model: current?.model ?? null,
			effort: current?.effort ?? null,
			mode: current?.mode ?? null,
			...change,
		};
		queryClient.setQueryData<AgentDefaultsList>(key, {
			items: [
				...(list?.items ?? []).filter((item) => item.agent !== agent),
				{ agent, ...next, updatedAt: new Date().toISOString() },
			],
		});
		save.mutate(
			{ agent, data: next },
			{ onSettled: () => void queryClient.invalidateQueries({ queryKey: key }) },
		);
	};
}

function submissionId(): string {
	// Web Crypto's random source is available on plain HTTP LAN servers too.
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
	bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
	const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function readChatCommands(chat: string, signal: AbortSignal) {
	return listChatCommands(chat, { signal, timeoutMs: 35_000 });
}

function draftContent(message: SendMessage): string {
	return JSON.stringify([
		message.text,
		message.attachments ?? [],
		message.references ?? [],
		message.command,
	]);
}

/** What a message shows as while it is sent: its text, and its command's arguments. */
function outgoingText(message: SendMessage): string {
	return [
		message.text,
		...Object.entries(message.command?.arguments ?? {}).map(
			([name, value]) => name + " = " + value,
		),
	]
		.filter(Boolean)
		.join("\n");
}

/**
 * A message on its way, shown in the conversation as "sending" until the
 * agent's echo of it (a user message with its `submissionId`) arrives.
 */
export interface Outgoing {
	text: string;
	/** Absent where the server takes no submission IDs. */
	submissionId?: string;
	/** The server took it: only the echo is awaited now. */
	sent: boolean;
}

/** How long a sent message waits for its echo before it stops showing as sending. */
const ECHO_WAIT_MS = 30_000;

export function useSendMessage(chat: string, supported: boolean) {
	const refresh = useRefresh();
	const [outgoing, setOutgoing] = useState<Outgoing>();
	const showing = useRef<Outgoing | undefined>(undefined);
	const show = useCallback((next: Outgoing | undefined) => {
		showing.current = next;
		setOutgoing(next);
	}, []);
	const [recovered, setRecovered] = useState(() => pendingChatMessage(chat));
	const pending = useRef<PendingMessage | undefined>(recovered);
	const remember = (message: PendingMessage | undefined) => {
		pending.current = message;
		rememberPendingChatMessage(chat, message);
	};
	const send = useMutation<void, ApiError, SendMessage>({
		mutationFn: async (data) => {
			const previous = pending.current;
			const retry =
				previous && draftContent(previous) === draftContent(data) ? previous : undefined;
			const id = retry?.submissionId ?? (supported ? submissionId() : undefined);
			const text = outgoingText(data);
			const mine: Outgoing | undefined = text ? { text, submissionId: id, sent: false } : undefined;
			show(mine);
			try {
				await deliver(data, retry, previous, id);
			} catch (error) {
				if (showing.current === mine) show(undefined);
				throw error;
			}
			// Without an ID nothing can match its echo; the conversation shows it soon.
			if (showing.current === mine) show(mine && id ? { ...mine, sent: true } : undefined);
		},
		onSuccess: refresh,
	});
	async function deliver(
		data: SendMessage,
		retry: PendingMessage | undefined,
		previous: PendingMessage | undefined,
		id: string | undefined,
	) {
		if (retry) {
			// Verify before retrying in case the server rolled back to a release
			// that would ignore the submission ID and send the message again.
			const current = await getChat(chat, { timeoutMs: 10_000 });
			if (!current.messageSubmissions)
				throw new ApiError(409, undefined, t("sessions:chatView.retryUnavailable"));
		} else if (!supported) {
			return sendMessage(chat, data, { timeoutMs: 130_000 });
		}
		// Work may have started or finished meanwhile. Retrying keeps the
		// original delivery choice as well as the submission ID.
		const request = retry ?? { ...data, submissionId: id ?? submissionId() };
		remember(request);
		if (request !== previous) setRecovered(undefined);
		await sendMessage(chat, request, { timeoutMs: 130_000 });
		if (pending.current === request) {
			remember(undefined);
			setRecovered(undefined);
		}
	}
	// An agent that never echoes leaves nothing "sending" for long.
	const waiting = outgoing?.sent ? outgoing : undefined;
	useEffect(() => {
		if (!waiting) return;
		const timer = setTimeout(() => {
			if (showing.current === waiting) show(undefined);
		}, ECHO_WAIT_MS);
		return () => clearTimeout(timer);
	}, [waiting, show]);
	/** The echo arrived (or it went to the queue): it no longer shows as sending. */
	const settleOutgoing = useCallback(
		(id: string) => {
			if (showing.current?.submissionId === id) show(undefined);
		},
		[show],
	);
	const dismissSubmission = () => {
		remember(undefined);
		setRecovered(undefined);
		send.reset();
	};
	return { ...send, recovered, dismissSubmission, outgoing, settleOutgoing };
}

/** Changes what a chat's agent may do without asking. */
export function useSetMode() {
	return useSetChatMode({ mutation: { onSuccess: useRefresh() } });
}

export function useInterruptChat() {
	return useGeneratedInterruptChat({ mutation: { onSuccess: useRefresh() } });
}

/** Sends a queued message next, stopping the turn; the chat's stream tells what waits now. */
export function useSendQueuedNow() {
	return useGeneratedSendQueuedNow();
}

/** Takes back a queued message; the chat's stream tells what waits now. */
export function useUnqueueMessage() {
	return useGeneratedUnqueueMessage();
}

/** Puts a chat's waiting messages in a new order. */
export function useReorderQueue() {
	return useGeneratedReorderQueue();
}

/** Changes the text of a waiting message; its files and context stay. */
export function useEditQueuedMessage() {
	return useGeneratedEditQueuedMessage();
}

export function useAnswerRequest(chatId: string) {
	const queryClient = useQueryClient();
	return useGeneratedAnswerRequest({
		mutation: {
			onSuccess: () => queryClient.invalidateQueries({ queryKey: getGetChatQueryKey(chatId) }),
		},
	});
}

/**
 * Searches a project's sessions (titles and messages) for every word of
 * `query`; nothing is asked while it is empty. The last results stay while
 * the next are asked for.
 */
export function useSessionSearch(project: string, query: string) {
	const q = query.trim();
	return useSearchSessions(
		project,
		{ q },
		{ query: { enabled: q.length > 0, placeholderData: (previous) => previous, staleTime: 5_000 } },
	);
}

/** After a change to a session's chats. */
function useChatsRefresh() {
	return useInvalidate("/sessions", "/chats");
}

/** Starts another chat in a session. */
export function useStartChatIn() {
	return useGeneratedStartChat({ mutation: { onSuccess: useChatsRefresh() } });
}

/** Renames a chat, or moves it to another session. */
export function useUpdateChat() {
	return useGeneratedUpdateChat({ mutation: { onSuccess: useChatsRefresh() } });
}

/** Stops a chat's agent; the chat stays, with its history. */
export function useCloseChat() {
	return useGeneratedCloseChat({ mutation: { onSuccess: useChatsRefresh() } });
}

/** Removes a stopped chat from its session. */
export function useRemoveChat() {
	return useGeneratedRemoveChat({ mutation: { onSuccess: useChatsRefresh() } });
}

/** A chat's model and effort, and the ones it goes on with at its next message. */
export function useChatModel(chatId: string) {
	return useGetChatModel(chatId, { query: { retry: false } });
}

/**
 * What a chat's agent has spent over the whole session (not only the turns
 * loaded here); asked again as each turn's accounting arrives (`after`).
 */
export function useChatUsage(chatId: string, after: string | undefined) {
	const usage = useGetChatUsage(chatId, { query: { retry: false, staleTime: 60_000 } });
	const { refetch } = usage;
	useEffect(() => {
		if (after) void refetch();
	}, [after, refetch]);
	return usage.data;
}

/**
 * Every turn of a chat, whatever part of its history is loaded; asked again
 * when a newer message of the person's is (`latest`, its place in the history).
 */
export function useChatTurns(chatId: string, latest: number | undefined) {
	const turns = useGetChatTurns(chatId, { query: { retry: false, staleTime: 60_000 } });
	const { refetch } = turns;
	useEffect(() => {
		if (latest !== undefined) void refetch();
	}, [latest, refetch]);
	return turns.data?.items;
}

/** Chooses another model or effort of the chat's own agent. */
export function useSetChatModel(chatId: string) {
	return useGeneratedSetChatModel({
		mutation: { onSuccess: useInvalidate(`/chats/${chatId}`) },
	});
}

/** Stops native work and returns the provider receipt. */
export function useStopWork() {
	return useStopChatWork();
}

export function useNativeModes(chatId: string, enabled: boolean) {
	return useGetChatModes(chatId, { query: { enabled, retry: false } });
}

export function readProjectCommands(project: string, agent: string, signal: AbortSignal) {
	return listProjectCommands(project, agent, { signal, timeoutMs: 90000 });
}
