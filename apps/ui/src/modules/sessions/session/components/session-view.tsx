import { cn } from "cn";
import { Bot, SquareTerminal } from "lucide-react";
import { useState } from "react";

import { ChatView, StartForm, useStartChatIn } from "@/modules/sessions/chat";
import { draftStore } from "@/modules/sessions/reference";
import { useDeleteSession, useSession, useUnarchiveSession } from "@/modules/sessions/session/api";
import { DRAGGED_PART } from "@/modules/sessions/session/components/part-tabs";
import { showPart, useSessionParts } from "@/modules/sessions/session/hooks";
import { type Part, partTitle } from "@/modules/sessions/session/model";
import { TerminalView } from "@/modules/sessions/terminal";
import type { Session, Project } from "@/shared/api/generated/ahab.schemas";
import { useChatContext } from "@/shared/chat-context/context";
import type { ContextReference } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { toastError } from "@/shared/ui/toast";

/**
 * A session's window: a tab for each of its chats and terminals,
 * one in front, perhaps another beside it, and `+` for another chat or
 * terminal in the same session. Each tab shows its chat or terminal as its
 * own window used to; they know nothing of each other.
 */
interface SessionViewProps {
	sessionId: string;
	/** The current project: paths in its folder show relative to it. */
	project?: Project;
	/** One pane at a time: no side by side. */
	phone?: boolean;
	/** Whether terminals may be started (the Terminal feature is on). */
	terminals?: boolean;
	onOpenFileDiff?: (path: string) => Promise<void>;
	onOpenFile?: (path: string, line?: number) => void;
	/** It was deleted: its window closes. */
	onGone?: () => void;
	/** A message was sent or a chat started in it: its window stays (no longer a preview). */
	onActivity?: () => void;
}

export function SessionView(props: SessionViewProps) {
	const { t } = useTranslation("sessions");
	const session = useSession(props.sessionId);
	if (session.isError)
		return <p className="text-muted-foreground p-6 text-sm">{t("sessionView.gone")}</p>;
	if (!session.data) return null;
	return <SessionBody {...props} data={session.data} />;
}

function SessionBody({
	data,
	project,
	phone,
	terminals = true,
	onOpenFile,
	onOpenFileDiff,
	onGone,
	onActivity,
}: Omit<SessionViewProps, "sessionId"> & { data: Session }) {
	const sessionId = data.id;
	const [dropping, setDropping] = useState(false);
	const {
		parts,
		hiddenParts,
		active,
		beside,
		archived,
		newChat,
		select,
		besideIt,
		openNewChat,
		newTerminal,
	} = useSessionParts(data, phone, terminals);
	const root = project?.id === data.projectId ? project.path : undefined;
	const chatContext = useChatContext();
	// A fork: the New chat tab (or a New session window), its composer holding
	// the history, any agent and model to choose; a fork before replaces it.
	const fork = (history: ContextReference, newSession: boolean) => {
		if (newSession) return chatContext?.startNew([history]);
		const key = newChatDraft(sessionId);
		const earlier = draftStore.get(key).filter((item) => item.kind === "chatHistory");
		draftStore.remove(
			key,
			earlier.map((item) => item.id),
		);
		draftStore.add(key, [history]);
		openNewChat();
	};

	const pane = (part: Part) =>
		part.kind === "chat" ? (
			<ChatView
				onFork={fork}
				onOpenFileDiff={onOpenFileDiff}
				onSent={onActivity}
				key={part.key}
				chatId={part.chat.id}
				sessionId={sessionId}
				archived={archived}
				root={root}
				onOpenFile={onOpenFile}
				archivedActions={
					archived ? (
						<ArchivedActions sessionId={sessionId} title={data.title} onGone={onGone} />
					) : undefined
				}
			/>
		) : (
			<TerminalView key={part.key} terminal={part.terminal} root={root} onOpenFile={onOpenFile} />
		);

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div
				className="relative flex min-h-0 flex-1"
				onDragOver={(event) => {
					if (phone || !event.dataTransfer.types.includes(DRAGGED_PART)) return;
					event.preventDefault();
					const box = event.currentTarget.getBoundingClientRect();
					setDropping(event.clientX > box.left + box.width / 2);
				}}
				onDragLeave={(event) => {
					if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(false);
				}}
				onDrop={(event) => {
					const key = event.dataTransfer.getData(DRAGGED_PART);
					const part = parts.find((item) => item.key === key);
					setDropping(false);
					if (!part) return;
					event.preventDefault();
					const box = event.currentTarget.getBoundingClientRect();
					if (event.clientX > box.left + box.width / 2 && part.key !== active?.key) besideIt(part);
					else select(part);
				}}
			>
				{newChat ? (
					<NewChatPane
						session={data}
						project={project}
						onOpenFile={onOpenFile}
						onStarted={onActivity}
					/>
				) : active ? (
					<>
						<div className="min-w-0 flex-1">{pane(active)}</div>
						{beside ? (
							<div className="border-border min-w-0 flex-1 border-l">{pane(beside)}</div>
						) : null}
					</>
				) : (
					<EmptyPane
						hidden={hiddenParts}
						archived={archived}
						onShow={select}
						onNewChat={openNewChat}
						onNewTerminal={newTerminal}
					/>
				)}
				{dropping ? (
					<div
						aria-hidden
						className="bg-primary/10 border-primary/50 pointer-events-none absolute inset-y-2 right-2 w-[calc(50%-1rem)] rounded-lg border-2 border-dashed"
					/>
				) : null}
			</div>
		</div>
	);
}

/** Nothing shows: every tab was closed, or there is nothing yet. */
function EmptyPane({
	hidden,
	archived,
	onShow,
	onNewChat,
	onNewTerminal,
}: {
	hidden: Part[];
	archived: boolean;
	onShow: (part: Part) => void;
	onNewChat: () => void;
	onNewTerminal?: () => void;
}) {
	const { t } = useTranslation("sessions");
	return (
		<div className="flex flex-1 flex-col items-center justify-center gap-4 p-6 text-center">
			<p className="text-muted-foreground text-sm">
				{hidden.length ? t("sessionView.allClosed") : t("sessionView.empty")}
			</p>
			{hidden.length ? (
				<ul className="flex flex-wrap justify-center gap-2">
					{hidden.map((part) => (
						<li key={part.key}>
							<Button size="sm" variant="outline" onClick={() => onShow(part)}>
								{part.kind === "chat" ? <Bot /> : <SquareTerminal />}
								{partTitle(part)}
							</Button>
						</li>
					))}
				</ul>
			) : null}
			{archived ? null : (
				<div className="flex gap-2">
					<Button size="sm" onClick={onNewChat}>
						<Bot />
						{t("sessionView.newChat")}
					</Button>
					{onNewTerminal ? (
						<Button size="sm" variant="outline" onClick={onNewTerminal}>
							<SquareTerminal />
							{t("sessionView.newTerminal")}
						</Button>
					) : null}
				</div>
			)}
		</div>
	);
}

/** The New chat tab's context draft (a fork's history). */
function newChatDraft(session: string): string {
	return `newChat:${session}`;
}

/** The New chat tab: the same form as a new session's, filed in this one. */
function NewChatPane({
	session,
	project,
	onOpenFile,
	onStarted,
}: {
	session: Session;
	project?: Project;
	onOpenFile?: (path: string, line?: number) => void;
	onStarted?: () => void;
}) {
	const { t } = useTranslation("sessions");
	const start = useStartChatIn();
	const here = project?.id === session.projectId ? project : undefined;
	return (
		<div className={cn("flex flex-1 items-start justify-center overflow-auto p-4 pt-[8vh] md:p-8")}>
			<div className="w-full max-w-2xl">
				<h2 className="font-heading text-heading mb-1 text-lg font-semibold">
					{t("sessionView.newChat")}
				</h2>
				<p className="text-muted-foreground mb-4 text-sm">
					{t("sessionView.newChatIn", { session: session.title })}
				</p>
				{here ? (
					<StartForm
						projectId={here.id}
						machineId={here.machineId}
						machineName={here.machineName}
						offline={here.availability !== "available"}
						pending={start.isPending}
						error={start.error?.message}
						draftKey={newChatDraft(session.id)}
						onOpenFile={onOpenFile}
						onStart={(chat) => {
							onStarted?.();
							return start.mutateAsync(
								{ id: session.id, data: chat },
								{
									onSuccess: (started) => {
										draftStore.clear(newChatDraft(session.id));
										showPart(session.id, `chat:${started.id}`);
									},
								},
							);
						}}
					/>
				) : (
					<p className="text-muted-foreground text-sm">{t("sessionView.otherProject")}</p>
				)}
			</div>
		</div>
	);
}

/** An archived session's way back, or out: Unarchive, Delete forever (asked first). */
function ArchivedActions({
	sessionId,
	title,
	onGone,
}: {
	sessionId: string;
	title: string;
	onGone?: () => void;
}) {
	const { t } = useTranslation("sessions");
	const unarchive = useUnarchiveSession();
	const remove = useDeleteSession();
	const [asking, setAsking] = useState(false);
	return (
		<>
			<Button
				size="sm"
				variant="outline"
				disabled={unarchive.isPending}
				onClick={() => unarchive.mutate({ id: sessionId }, { onError: toastError })}
			>
				{t("sessionView.unarchive")}
			</Button>
			<Button
				size="sm"
				variant="ghost"
				className="text-destructive"
				onClick={() => setAsking(true)}
			>
				{t("sessionView.deleteForever")}
			</Button>
			<ConfirmDialog
				open={asking}
				onOpenChange={setAsking}
				title={t("sessionView.deleteTitle", { title })}
				description={t("sessionView.deleteDescription")}
				action={t("sessionView.deleteForever")}
				destructive
				pending={remove.isPending}
				error={remove.error?.message}
				onConfirm={() =>
					remove.mutate(
						{ id: sessionId },
						{
							onSuccess: () => {
								setAsking(false);
								onGone?.();
							},
						},
					)
				}
			/>
		</>
	);
}
