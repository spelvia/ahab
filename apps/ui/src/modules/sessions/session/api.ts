import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useSyncExternalStore } from "react";

import { onEvent, onEventConnection } from "@/shared/api/events";
import {
	getGetSessionQueryKey,
	getListSessionsQueryKey,
	getWaitingSessionsQueryKey,
	useArchiveSession as useGeneratedArchiveSession,
	useCreateSessionFolder as useGeneratedCreateFolder,
	useCreateSession as useGeneratedCreateSession,
	useDeleteSessionFolder as useGeneratedDeleteFolder,
	useDeleteSession as useGeneratedDeleteSession,
	useGetSession,
	useListSessionFolders,
	useListSessions,
	useRenameSessionFolder as useGeneratedRenameFolder,
	useUnarchiveSession as useGeneratedUnarchiveSession,
	useUpdateSession as useGeneratedUpdateSession,
	useWaitingSessions as useGeneratedWaitingSessions,
} from "@/shared/api/generated/sessions-session/sessions-session";
import { useInvalidate } from "@/shared/api/invalidate";

let live = false;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
	listeners.add(listener);
	return () => void listeners.delete(listener);
};
const isLive = () => live;
const setLive = (ready: boolean) => {
	if (ready === live) return;
	live = ready;
	for (const listener of listeners) listener();
};

/** Refresh affected queries at once, retaining a poll for gaps and older servers. */
export function useSessionUpdates() {
	const client = useQueryClient();
	useEffect(() => {
		const dirty = new Set<string>();
		let timer: ReturnType<typeof setTimeout> | undefined;
		const waiting = getWaitingSessionsQueryKey()[0];
		const close = onEventConnection((open) => {
			if (!open) setLive(false);
		});
		const ready = onEvent("stream.ready", (data) => {
			if (!(data as { sessions?: boolean } | null)?.sessions) return;
			setLive(true);
			void client.invalidateQueries({
				predicate: ({ queryKey: [key] }) =>
					typeof key === "string" &&
					/^\/api\/v1\/(projects\/[^/]+\/sessions|sessions\/[^/]+)$/.test(key),
			});
		});
		const changed = onEvent("sessions.changed", (data) => {
			const event = data as { project?: unknown; session?: unknown } | null;
			if (typeof event?.project !== "string" || typeof event.session !== "string") return;
			dirty.add(getListSessionsQueryKey(event.project)[0]);
			dirty.add(getGetSessionQueryKey(event.session)[0]);
			dirty.add(waiting);
			timer ??= setTimeout(() => {
				timer = undefined;
				void client.invalidateQueries({
					predicate: ({ queryKey: [key] }) => typeof key === "string" && dirty.has(key),
				});
				dirty.clear();
			}, 80);
		});
		return () => {
			clearTimeout(timer);
			close();
			ready();
			changed();
			setLive(false);
		};
	}, [client]);
}

function useSessionPoll(fallback: number) {
	return useSyncExternalStore(subscribe, isLive, () => false) ? 30_000 : fallback;
}

/** A project's sessions with their chats. */
export function useSessions(project: string) {
	return useListSessions(project, { query: { refetchInterval: useSessionPoll(2_500) } });
}

/** One session with its chats. */
export function useSession(id: string) {
	return useGetSession(id, {
		query: { enabled: Boolean(id), refetchInterval: useSessionPoll(2_500), retry: false },
	});
}

/** How many sessions need you, per project. */
export function useWaitingSessions() {
	return useGeneratedWaitingSessions({ query: { refetchInterval: useSessionPoll(4_000) } });
}

function useRefresh() {
	// Sessions, their chats and folders, and the waiting counts.
	return useInvalidate("/sessions", "/chats", "/session-folders");
}

export function useCreateSession() {
	return useGeneratedCreateSession({ mutation: { onSuccess: useRefresh() } });
}

/** Renames, pins or files a session. */
export function useUpdateSession() {
	return useGeneratedUpdateSession({ mutation: { onSuccess: useRefresh() } });
}

/** Ends a session's chats and lists it apart. */
export function useArchiveSession() {
	return useGeneratedArchiveSession({ mutation: { onSuccess: useRefresh() } });
}

export function useUnarchiveSession() {
	return useGeneratedUnarchiveSession({ mutation: { onSuccess: useRefresh() } });
}

/** Deletes an archived session with its history. */
export function useDeleteSession() {
	return useGeneratedDeleteSession({ mutation: { onSuccess: useRefresh() } });
}

/** A project's folders of sessions, by name. */
export function useFolders(project: string) {
	return useListSessionFolders(project);
}

export function useCreateFolder() {
	return useGeneratedCreateFolder({ mutation: { onSuccess: useRefresh() } });
}

export function useRenameFolder() {
	return useGeneratedRenameFolder({ mutation: { onSuccess: useRefresh() } });
}

/** Deletes a folder; its sessions stay, out of any folder. */
export function useDeleteFolder() {
	return useGeneratedDeleteFolder({ mutation: { onSuccess: useRefresh() } });
}
