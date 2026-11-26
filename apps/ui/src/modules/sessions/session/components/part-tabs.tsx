import { cn } from "cn";
import {
	Bot,
	ChevronDown,
	Columns2,
	FolderInput,
	MoreHorizontal,
	Pencil,
	Plus,
	Square,
	SquareTerminal,
	Trash2,
	X,
} from "lucide-react";
import { type ReactNode, useState } from "react";

import { AgentIcon } from "@/modules/machines";
import { useCloseChat, useRemoveChat, useUpdateChat } from "@/modules/sessions/chat";
import { useSessions } from "@/modules/sessions/session/api";
import { useSessionParts } from "@/modules/sessions/session/hooks";
import { type Part, partLive, partState, partTitle } from "@/modules/sessions/session/model";
import {
	shownTitle,
	useKillTerminal,
	useRemoveTerminal,
	useUpdateTerminal,
} from "@/modules/sessions/terminal";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";
import { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { toastError } from "@/shared/ui/toast";

/** The drag payload of a session's tab: its key, to show it beside another. */
export const DRAGGED_PART = "application/x-ahab-session-part";

/** What a tab's menu can ask for. */
export interface PartActions {
	onSelect: (part: Part) => void;
	onClose: (part: Part) => void;
	/** Shows it beside the tab in front; absent where there is no room (phones). */
	onBeside?: (part: Part) => void;
	onAlone?: () => void;
}

export function SessionPartPicker({
	session,
	children,
	onActivate,
	phone,
	terminals,
}: {
	session: Session;
	children?: ReactNode;
	onActivate?: () => void;
	phone?: boolean;
	terminals?: boolean;
}) {
	const { t } = useTranslation("sessions");
	const {
		shown,
		active,
		beside,
		newChat,
		select,
		close,
		besideIt,
		alone,
		archived,
		openNewChat,
		newTerminal,
	} = useSessionParts(session, phone, terminals);
	const [open, setOpen] = useState(false);
	const name = shownTitle(session);
	// How many tabs it holds: shown when more than one, so none hide unnoticed.
	const count = shown.length + (newChat ? 1 : 0);
	const label = t(open ? "sessionView.hideTabs" : "sessionView.showTabs", { name, count });
	const dismiss = () => setOpen(false);
	const activate = () => {
		dismiss();
		onActivate?.();
	};
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverAnchor asChild>
				<div className="flex h-full min-w-0 items-center" onContextMenuCapture={dismiss}>
					{children && (
						<div className="flex h-full min-w-0 flex-1 items-center" onClick={dismiss}>
							{children}
						</div>
					)}
					<PopoverTrigger asChild>
						<button
							type="button"
							aria-label={label}
							onPointerDown={(event) => event.stopPropagation()}
							onMouseDown={(event) => event.stopPropagation()}
							onClick={(event) => event.stopPropagation()}
							title={label}
							className={cn(
								"text-muted-foreground hover:bg-accent focus-visible:ring-ring flex h-7 min-w-7 shrink-0 items-center justify-center gap-0.5 rounded outline-none focus-visible:ring-2",
								count > 1 && "px-1",
							)}
						>
							{count > 1 ? (
								<span className="bg-muted text-foreground min-w-4 rounded px-1 text-[10px] leading-4 font-medium tabular-nums">
									{count}
								</span>
							) : null}
							<ChevronDown className={cn("size-3.5", open && "rotate-180")} />
						</button>
					</PopoverTrigger>
				</div>
			</PopoverAnchor>
			<PopoverContent
				aria-label={name}
				align="start"
				sideOffset={3}
				className="max-h-[var(--radix-popover-content-available-height)] max-w-[calc(100vw-1rem)] overflow-y-auto p-1"
			>
				<div className="flex items-center justify-between">
					<p className="text-muted-foreground truncate px-2 py-1.5 text-xs font-medium">
						{shownTitle(session)}
					</p>
					{archived ? null : (
						<DropdownMenu>
							<DropdownMenuTrigger
								aria-label={t("sessionView.add")}
								title={t("sessionView.add")}
								className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring mx-1 my-auto grid size-7 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2"
							>
								<Plus className="size-4" />
							</DropdownMenuTrigger>
							<DropdownMenuContent align="start">
								<DropdownMenuItem
									onSelect={() => {
										openNewChat();
										activate();
									}}
								>
									<Bot />
									{t("sessionView.newChat")}
								</DropdownMenuItem>
								{newTerminal ? (
									<DropdownMenuItem
										onSelect={() => {
											newTerminal();
											activate();
										}}
									>
										<SquareTerminal />
										{t("sessionView.newTerminal")}
									</DropdownMenuItem>
								) : null}
							</DropdownMenuContent>
						</DropdownMenu>
					)}
				</div>
				{shown.map((part) => (
					<PartTab
						key={part.key}
						session={session}
						part={part}
						active={active?.key === part.key}
						beside={beside?.key === part.key}
						actions={{
							onSelect: (picked) => {
								select(picked);
								activate();
							},
							onBeside: phone
								? undefined
								: (picked) => {
										besideIt(picked);
										dismiss();
										onActivate?.();
									},
							onAlone: alone,
							onClose: close,
						}}
					/>
				))}
				{newChat && (
					<button
						type="button"
						aria-current="true"
						className="bg-accent text-heading flex w-full items-center gap-2 rounded px-2 py-2 text-sm"
						onClick={() => {
							dismiss();
							onActivate?.();
						}}
					>
						<Bot className="size-4" />
						{t("sessionView.newChat")}
					</button>
				)}
				{!shown.length && !newChat && (
					<p className="text-muted-foreground px-2 py-2 text-sm">{t("sessionView.noOpenTabs")}</p>
				)}
			</PopoverContent>
		</Popover>
	);
}

function PartIcon({ part, className }: { part: Part; className?: string }) {
	return part.kind === "chat" ? (
		<AgentIcon agent={part.chat.agent} className={className} />
	) : (
		<SquareTerminal className={className} aria-hidden />
	);
}

function PartTab({
	session,
	part,
	active,
	beside,
	actions,
}: {
	session: Session;
	part: Part;
	active: boolean;
	beside: boolean;
	actions: PartActions;
}) {
	const { t } = useTranslation("sessions");
	const [dialog, setDialog] = useState<"rename" | "remove" | "move" | null>(null);
	const [confirmClose, setConfirmClose] = useState(false);
	const [menu, setMenu] = useState(false);
	const state = partState(part);
	const title = partTitle(part);
	const requestClose = () => {
		if (part.kind === "terminal") setDialog("remove");
		else setConfirmClose(true);
	};
	const menuProps = {
		session,
		part,
		beside,
		actions: { ...actions, onClose: requestClose },
		onDialog: setDialog,
	};

	return (
		<>
			<ContextMenu onOpenChange={setMenu}>
				<ContextMenuTrigger asChild>
					<div
						draggable
						onDragStart={(event) => {
							event.dataTransfer.setData(DRAGGED_PART, part.key);
							event.dataTransfer.effectAllowed = "move";
						}}
						className={cn(
							"relative flex min-w-0 items-center gap-1.5 rounded pr-1 pl-2 text-[0.8125rem]",
							active
								? "bg-accent text-heading"
								: beside
									? "bg-background/60 text-foreground"
									: "text-muted-foreground hover:text-foreground hover:bg-accent/40",
							menu && "bg-accent/40",
						)}
					>
						<button
							type="button"
							aria-current={active ? "true" : undefined}
							onClick={() => actions.onSelect(part)}
							title={`${title} · ${state.label}`}
							className="focus-visible:ring-ring flex min-w-0 flex-1 items-center gap-1.5 rounded py-1.5 text-left outline-none focus-visible:ring-2"
						>
							<PartIcon part={part} className="size-3.5 shrink-0" />
							<span className="truncate">{title}</span>
							<span className={cn("size-1.5 shrink-0 rounded-full", state.dot)} aria-hidden />
							<span className="sr-only">{state.label}</span>
						</button>
						<DropdownMenu>
							<DropdownMenuTrigger
								aria-label={t("sessionView.tabActions", { name: title })}
								title={t("sessionView.tabActions", { name: title })}
								className="hover:bg-accent text-muted-foreground hover:text-foreground grid size-6 shrink-0 place-items-center rounded"
							>
								<MoreHorizontal className="size-3.5" />
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end">
								<PartMenuItems
									{...menuProps}
									Item={DropdownMenuItem}
									Separator={DropdownMenuSeparator}
								/>
							</DropdownMenuContent>
						</DropdownMenu>
					</div>
				</ContextMenuTrigger>
				<ContextMenuContent>
					<PartMenuItems {...menuProps} Item={ContextMenuItem} Separator={ContextMenuSeparator} />
				</ContextMenuContent>
			</ContextMenu>
			<ConfirmDialog
				open={confirmClose}
				onOpenChange={setConfirmClose}
				title={t("sessionView.confirmCloseTitle", { name: title })}
				description={t("sessionView.confirmCloseChat")}
				action={t("sessionView.closeTabTitle")}
				onConfirm={() => actions.onClose(part)}
			/>
			<PartDialogs session={session} part={part} dialog={dialog} onDone={() => setDialog(null)} />
		</>
	);
}

/**
 * A chat's or terminal's actions: Rename, Show beside, Stop, Remove (once
 * stopped), Move to session, Close tab. In a tab's menu and in the tree's.
 */
export function PartMenuItems({
	session,
	part,
	beside,
	actions,
	onDialog,
	Item,
	Separator,
}: {
	session: Session;
	part: Part;
	beside?: boolean;
	actions?: PartActions;
	onDialog: (dialog: "rename" | "remove" | "move") => void;
	Item: typeof ContextMenuItem | typeof DropdownMenuItem;
	Separator: typeof ContextMenuSeparator;
}) {
	const { t } = useTranslation("sessions");
	const closeChat = useCloseChat();
	const stopTerminal = useKillTerminal();
	const live = partLive(part);
	const archived = Boolean(session.archivedAt);
	const stop = () => {
		if (part.kind === "chat") closeChat.mutate({ id: part.chat.id }, { onError: toastError });
		else stopTerminal.mutate({ id: part.terminal.id }, { onError: toastError });
	};
	return (
		<>
			<Item onSelect={() => onDialog("rename")}>
				<Pencil />
				{t("sessionView.rename")}
			</Item>
			{actions?.onBeside && !beside ? (
				<Item onSelect={() => actions.onBeside?.(part)}>
					<Columns2 />
					{t("sessionView.showBeside")}
				</Item>
			) : null}
			{actions?.onAlone && beside ? (
				<Item onSelect={actions.onAlone}>
					<Columns2 />
					{t("sessionView.showAlone")}
				</Item>
			) : null}
			{archived ? null : (
				<Item onSelect={() => onDialog("move")}>
					<FolderInput />
					{t("sessionView.move")}
				</Item>
			)}
			<Separator />
			{live ? (
				<Item onSelect={stop}>
					<Square />
					{t("sessionView.stop")}
				</Item>
			) : (
				<Item onSelect={() => onDialog("remove")}>
					<Trash2 />
					{t("sessionView.remove")}
				</Item>
			)}
			{actions ? (
				<Item onSelect={() => actions.onClose(part)}>
					<X />
					{t("actions.close", { ns: "common" })}
				</Item>
			) : null}
		</>
	);
}

/** The dialogs a chat's or terminal's actions open. */
export function PartDialogs({
	session,
	part,
	dialog,
	onDone,
}: {
	session: Session;
	part: Part;
	dialog: "rename" | "remove" | "move" | null;
	onDone: () => void;
}) {
	const { t } = useTranslation("sessions");
	const renameChat = useUpdateChat();
	const renameTerminal = useUpdateTerminal();
	const removeChat = useRemoveChat();
	const removeTerminal = useRemoveTerminal();
	const title = partTitle(part);
	const renaming = part.kind === "chat" ? renameChat : renameTerminal;
	const removing = part.kind === "chat" ? removeChat : removeTerminal;
	return (
		<>
			<NameDialog
				open={dialog === "rename"}
				onOpenChange={(open) => {
					if (!open) {
						renaming.reset();
						onDone();
					}
				}}
				title={t("sessionView.renameTitle")}
				label={t("sessionView.renameLabel")}
				initial={part.kind === "chat" ? part.chat.title : (part.terminal.label ?? "")}
				action={t("sessionView.renameAction")}
				pending={renaming.isPending}
				error={renaming.error?.message}
				onSubmit={(name) => {
					if (part.kind === "chat")
						renameChat.mutate({ id: part.chat.id, data: { title: name } }, { onSuccess: onDone });
					else
						renameTerminal.mutate(
							{ id: part.terminal.id, data: { label: name } },
							{ onSuccess: onDone },
						);
				}}
			/>
			<ConfirmDialog
				open={dialog === "remove"}
				onOpenChange={(open) => {
					if (!open) {
						removing.reset();
						onDone();
					}
				}}
				title={t("sessionView.removeTitle", { name: title })}
				description={
					part.kind === "chat" ? t("sessionView.removeChat") : t("sessionView.removeTerminal")
				}
				action={t("sessionView.removeAction")}
				destructive
				pending={removing.isPending}
				error={removing.error?.message}
				onConfirm={() => {
					if (part.kind === "chat") removeChat.mutate({ id: part.chat.id }, { onSuccess: onDone });
					else
						removeTerminal.mutate(
							{ id: part.terminal.id, params: { stop: true } },
							{ onSuccess: onDone },
						);
				}}
			/>
			{dialog === "move" ? <MoveDialog session={session} part={part} onDone={onDone} /> : null}
		</>
	);
}

/** Another session of the same project to move a chat or terminal to. */
function MoveDialog({
	session,
	part,
	onDone,
}: {
	session: Session;
	part: Part;
	onDone: () => void;
}) {
	const { t } = useTranslation("sessions");
	const sessions = useSessions(session.projectId);
	const moveChat = useUpdateChat();
	const moveTerminal = useUpdateTerminal();
	const moving = part.kind === "chat" ? moveChat : moveTerminal;
	const others = (sessions.data?.items ?? []).filter(
		(other) =>
			other.id !== session.id && other.projectId === session.projectId && !other.archivedAt,
	);
	const move = (target: Session) => {
		const done = { onSuccess: onDone };
		if (part.kind === "chat")
			moveChat.mutate({ id: part.chat.id, data: { sessionId: target.id } }, done);
		else moveTerminal.mutate({ id: part.terminal.id, data: { sessionId: target.id } }, done);
	};
	return (
		<Dialog open onOpenChange={(open) => (open ? null : onDone())}>
			<DialogContent size="md">
				<DialogHeader>
					<DialogTitle>{t("sessionView.moveTitle", { name: partTitle(part) })}</DialogTitle>
					<DialogDescription>{t("sessionView.moveDescription")}</DialogDescription>
				</DialogHeader>
				<DialogBody className="px-4">
					{others.length === 0 ? (
						<p className="text-muted-foreground px-2 text-sm">{t("sessionView.moveNone")}</p>
					) : (
						<ul className="flex flex-col">
							{others.map((other) => (
								<li key={other.id}>
									<button
										type="button"
										disabled={moving.isPending}
										onClick={() => move(other)}
										className="hover:bg-accent focus-visible:bg-accent flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm outline-none"
									>
										<MoreHorizontal
											className="text-muted-foreground size-4 rotate-90"
											aria-hidden
										/>
										<span className="min-w-0 flex-1 truncate">{shownTitle(other)}</span>
									</button>
								</li>
							))}
						</ul>
					)}
					{moving.isError ? (
						<p role="alert" className="text-destructive px-2 text-sm">
							{moving.error.message}
						</p>
					) : null}
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}
