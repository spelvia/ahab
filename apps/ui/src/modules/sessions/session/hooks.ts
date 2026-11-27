import { useSyncExternalStore } from "react";

import { useArchiveSession, useUnarchiveSession } from "@/modules/sessions/session/api";
import { type Part, partsOf } from "@/modules/sessions/session/model";
import { useStartTerminalIn } from "@/modules/sessions/terminal";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";
import { rememberSessionView, type SessionView, sessionView } from "@/shared/preferences/store";
import { toast, toastError } from "@/shared/ui/toast";

/** Archives a session, then offers to undo it for a few seconds. */
export function useArchive() {
	const archive = useArchiveSession();
	const unarchive = useUnarchiveSession();
	return {
		pending: archive.isPending,
		error: archive.error?.message,
		run: (session: Session, onArchived?: () => void) =>
			archive.mutate(
				{ id: session.id },
				{
					onSuccess: () => {
						onArchived?.();
						toast(t("sessions:archive.done"), {
							duration: 6000,
							action: {
								label: t("actions.undo"),
								run: () => unarchive.mutate({ id: session.id }),
							},
						});
					},
				},
			),
	};
}

/**
 * Each session window's tabs: which is in front, which beside it,
 * which are closed. Kept on this device; the tree, notifications and
 * search choose the tab a window opens at.
 */
const views = new Map<string, SessionView>();
const listeners = new Set<() => void>();

function viewOf(session: string): SessionView {
	let view = views.get(session);
	if (!view) {
		view = sessionView(session);
		views.set(session, view);
	}
	return view;
}

/** Changes a session window's view, and keeps it. */
function changeSessionView(session: string, change: (view: SessionView) => SessionView) {
	const next = change(viewOf(session));
	views.set(session, next);
	rememberSessionView(session, next);
	for (const listener of listeners) listener();
}

/** Puts a chat's or terminal's tab in front of its session's window, closed or not. */
export function showPart(session: string, key: string) {
	changeSessionView(session, (view) => ({
		...view,
		active: key,
		beside: view.beside === key ? undefined : view.beside,
		hidden: view.hidden?.filter((hidden) => hidden !== key),
	}));
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** A session window's view, re-rendering when it changes. */
function useSessionView(session: string): SessionView {
	return useSyncExternalStore(subscribe, () => viewOf(session));
}

export function useSessionParts(data: Session, phone?: boolean, terminals = true) {
	const sessionId = data.id;
	const startTerminal = useStartTerminalIn();
	const view = useSessionView(sessionId);
	const archived = Boolean(data.archivedAt);
	const parts = partsOf(data);
	const shown = parts.filter((part) => !view.hidden?.includes(part.key));
	const hiddenParts = parts.filter((part) => view.hidden?.includes(part.key));
	const newChat = view.active === "new" && !archived;
	const active = newChat
		? undefined
		: (shown.find((part) => part.key === view.active) ?? shown.at(-1));
	const beside =
		phone || newChat
			? undefined
			: shown.find((part) => part.key === view.beside && part.key !== active?.key);

	const newTerminal =
		terminals && !archived
			? () =>
					startTerminal.mutate(
						{ id: sessionId, data: { cols: 100, rows: 30 } },
						{
							onSuccess: (started) => showPart(sessionId, `terminal:${started.id}`),
							onError: toastError,
						},
					)
			: undefined;

	const select = (part: Part) => showPart(sessionId, part.key);
	const close = (part: Part) =>
		changeSessionView(sessionId, (current) => ({
			...current,
			hidden: [...(current.hidden ?? []).filter((key) => key !== part.key), part.key],
			active: current.active === part.key ? undefined : current.active,
			beside: current.beside === part.key ? undefined : current.beside,
		}));
	const besideIt = (part: Part) =>
		changeSessionView(sessionId, (current) => ({
			...current,
			// The one in front stays; the other shows beside it.
			active:
				active?.key === part.key ? shown.find((item) => item.key !== part.key)?.key : active?.key,
			beside: part.key,
		}));
	const alone = () =>
		changeSessionView(sessionId, (current) => ({ ...current, beside: undefined }));
	const openNewChat = () =>
		changeSessionView(sessionId, (current) => ({ ...current, active: "new" }));
	return {
		parts,
		shown,
		hiddenParts,
		active,
		beside,
		archived,
		newChat,
		select,
		close,
		besideIt,
		alone,
		openNewChat,
		newTerminal,
	};
}
