import { useCallback, useMemo, useState } from "react";

import { ChatChooser } from "@/modules/sessions/reference/components/chat-chooser";
import { draftStore, touchChatTarget, useChatTargetId } from "@/modules/sessions/reference/hooks";
import { useSession } from "@/modules/sessions/session";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { ChatContext, type ChatContextValue } from "@/shared/chat-context/context";
import { type ContextReference, referenceLabel } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { toast } from "@/shared/ui/toast";

/**
 * Adding context to chats, for everything inside: the target is
 * the project's session window used last; Add to chat puts references in
 * its composer, the chooser picks another chat or a new session.
 */
export function ChatContextProvider({
	projectId,
	openSessions,
	onShowSession,
	onNewSessionWith,
	children,
}: {
	projectId: string;
	/** Sessions with a window open, in tab order. */
	openSessions: string[];
	/** Brings a session's window forward (opening it if needed). */
	onShowSession: (session: { id: string; title: string }) => void;
	/** Opens a New session window whose composer holds the draft under `draftKey`. */
	onNewSessionWith: (draftKey: string) => void;
	children: React.ReactNode;
}) {
	const { t } = useTranslation(["sessions", "common"]);
	const targetId = useChatTargetId(projectId);
	const found = useSession(targetId ?? "");
	const session = found.data;
	const usable =
		session && session.kind === "chat" && !session.archivedAt && session.projectId === projectId;
	const usableId = usable ? session.id : undefined;
	const targetTitle = usable ? session.title : undefined;
	const [choosing, setChoosing] = useState<ContextReference[] | null>(null);

	const addTo = useCallback(
		(chosen: { id: string; title: string }, references: ContextReference[]) => {
			const added = draftStore.add(chosen.id, references);
			touchChatTarget(projectId, chosen.id);
			toast(
				added.length
					? t("chatContext.added", { title: chosen.title })
					: t("chatContext.alreadyThere", { title: chosen.title }),
				{
					actions: [
						{ label: t("chatContext.show"), run: () => onShowSession(chosen) },
						...(added.length
							? [
									{
										label: t("common:actions.undo"),
										run: () =>
											draftStore.remove(
												chosen.id,
												added.map((item) => item.id),
											),
									},
								]
							: []),
					],
				},
			);
		},
		[projectId, onShowSession, t],
	);

	const value = useMemo<ChatContextValue>(() => {
		const target =
			usableId && targetTitle !== undefined ? { sessionId: usableId, title: targetTitle } : null;
		return {
			target,
			add: (references) => {
				if (target) addTo({ id: target.sessionId, title: target.title }, references);
				else setChoosing(references);
			},
			choose: (references) => setChoosing(references),
			startNew: (references) => {
				const key = `new:${Date.now().toString(36)}`;
				draftStore.add(key, references);
				onNewSessionWith(key);
			},
		};
	}, [usableId, targetTitle, addTo, onNewSessionWith]);

	const what = choosing
		? choosing.length === 1 && choosing[0]
			? referenceLabel(choosing[0])
			: t("chatContext.pieces", { count: choosing.length })
		: "";
	return (
		<ChatContext.Provider value={value}>
			{children}
			{choosing ? (
				<ChatChooser
					open
					onOpenChange={(open) => {
						if (!open) setChoosing(null);
					}}
					projectId={projectId}
					openSessions={openSessions}
					what={what}
					onPick={(picked: Session) => addTo(picked, choosing)}
					onNew={() => {
						const key = `new:${Date.now().toString(36)}`;
						draftStore.add(key, choosing);
						onNewSessionWith(key);
					}}
				/>
			) : null}
		</ChatContext.Provider>
	);
}
