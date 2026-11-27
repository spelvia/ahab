import {
	Archive,
	ArchiveRestore,
	Folder,
	FolderMinus,
	FolderPlus,
	Bot,
	OctagonX,
	Pencil,
	Pin,
	PinOff,
	RotateCcw,
	SquareTerminal,
	Trash2,
} from "lucide-react";
import { useState } from "react";

import {
	useArchiveSession,
	useCreateFolder,
	useDeleteSession,
	useFolders,
	useUnarchiveSession,
	useUpdateSession,
} from "@/modules/sessions/session/api";
import { showPart, useArchive } from "@/modules/sessions/session/hooks";
import { isBusy } from "@/modules/sessions/session/model";
import {
	useKillTerminal,
	useRestartTerminal,
	useStartTerminalIn,
} from "@/modules/sessions/terminal";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";
import { toastError } from "@/shared/ui/toast";

/** What the session's menu needs to know and do. */
interface SessionMenuOptions {
	session: Session;
	onRename: () => void;
	/** It was archived or deleted: leave it if it is open. */
	onArchived?: () => void;
	onDeleted?: () => void;
	/** Opens the session's window: New chat here and New terminal here show there. */
	onOpen?: () => void;
	/** Whether terminals may be started (the Terminal feature is on). */
	terminals?: boolean;
}

/**
 * What can be done with a session, as menu items and the dialogs they open:
 * rename, pin, file in a folder, archive; archived ones are unarchived or
 * deleted. Archiving a session at work asks first, and can be undone. Used
 * by its ⋯ menu and by its tab's right-click menu.
 */
export function useSessionMenu({
	session,
	onRename,
	onArchived,
	onDeleted,
	onOpen,
	terminals = true,
}: SessionMenuOptions) {
	const { t } = useTranslation(["sessions", "common"]);
	const folders = useFolders(session.projectId);
	const update = useUpdateSession();
	const createFolder = useCreateFolder();
	const unarchive = useUnarchiveSession();
	const remove = useDeleteSession();
	const archiving = useArchiveSession();
	const archive = useArchive();
	const [dialog, setDialog] = useState<"archive" | "delete" | "folder" | null>(null);
	const archived = Boolean(session.archivedAt);
	// A session that is one terminal: its Restart and Stop are the session's.
	const terminal =
		session.chats.length === 0 && session.terminals.length === 1 ? session.terminals[0] : null;
	const restart = useRestartTerminal();
	const kill = useKillTerminal();
	const pinned = Boolean(session.pinnedAt);
	const close = () => setDialog(null);
	const startTerminal = useStartTerminalIn();
	// Another chat or terminal in this session, shown in its window.
	const adding = onOpen ? (
		<>
			<DropdownMenuItem
				onSelect={() => {
					showPart(session.id, "new");
					onOpen();
				}}
			>
				<Bot />
				{t("sessionView.newChatHere")}
			</DropdownMenuItem>
			{terminals ? (
				<DropdownMenuItem
					onSelect={() =>
						startTerminal.mutate(
							{ id: session.id, data: { cols: 100, rows: 30 } },
							{
								onSuccess: (started) => {
									showPart(session.id, `terminal:${started.id}`);
									onOpen();
								},
								onError: toastError,
							},
						)
					}
				>
					<SquareTerminal />
					{t("sessionView.newTerminalHere")}
				</DropdownMenuItem>
			) : null}
			<DropdownMenuSeparator />
		</>
	) : null;

	const file = (folderId: string | null) => update.mutate({ id: session.id, data: { folderId } });
	const startArchive = () => {
		if (isBusy(session)) setDialog("archive");
		else archive.run(session, onArchived);
	};

	const items = archived ? (
		<>
			<DropdownMenuItem onSelect={() => unarchive.mutate({ id: session.id })}>
				<ArchiveRestore />
				{t("sessionMenu.unarchive")}
			</DropdownMenuItem>
			<DropdownMenuSeparator />
			<DropdownMenuItem variant="destructive" onSelect={() => setDialog("delete")}>
				<Trash2 />
				{t("sessionMenu.delete")}
			</DropdownMenuItem>
		</>
	) : (
		<>
			{adding}
			{terminal ? (
				<>
					<DropdownMenuItem
						onSelect={() => restart.mutate({ id: terminal.id }, { onError: toastError })}
					>
						<RotateCcw />
						{t("sessionMenu.restart")}
					</DropdownMenuItem>
					{terminal.state === "running" ? (
						<DropdownMenuItem
							variant="destructive"
							onSelect={() => kill.mutate({ id: terminal.id }, { onError: toastError })}
						>
							<OctagonX />
							{t("sessionMenu.kill")}
						</DropdownMenuItem>
					) : null}
					<DropdownMenuSeparator />
				</>
			) : null}
			<DropdownMenuItem onSelect={onRename}>
				<Pencil />
				{t("sessionMenu.rename")}
			</DropdownMenuItem>
			<DropdownMenuItem
				onSelect={() => update.mutate({ id: session.id, data: { pinned: !pinned } })}
			>
				{pinned ? <PinOff /> : <Pin />}
				{pinned ? t("sessionMenu.unpin") : t("sessionMenu.pin")}
			</DropdownMenuItem>
			<DropdownMenuSub>
				<DropdownMenuSubTrigger>
					<Folder />
					{t("sessionMenu.moveToFolder")}
				</DropdownMenuSubTrigger>
				<DropdownMenuSubContent>
					{(folders.data?.items ?? []).map((folder) => (
						<DropdownMenuItem
							key={folder.id}
							disabled={folder.id === session.folderId}
							onSelect={() => file(folder.id)}
						>
							<Folder />
							<span className="truncate">{folder.name}</span>
						</DropdownMenuItem>
					))}
					{folders.data?.items.length ? <DropdownMenuSeparator /> : null}
					<DropdownMenuItem onSelect={() => setDialog("folder")}>
						<FolderPlus />
						{t("sessionMenu.newFolder")}
					</DropdownMenuItem>
					{session.folderId ? (
						<DropdownMenuItem onSelect={() => file(null)}>
							<FolderMinus />
							{t("sessionMenu.removeFromFolder")}
						</DropdownMenuItem>
					) : null}
				</DropdownMenuSubContent>
			</DropdownMenuSub>
			<DropdownMenuSeparator />
			<DropdownMenuItem onSelect={startArchive}>
				<Archive />
				{t("sessionMenu.archive")}
			</DropdownMenuItem>
			<DropdownMenuItem variant="destructive" onSelect={() => setDialog("delete")}>
				<Trash2 />
				{t("sessionMenu.delete")}
			</DropdownMenuItem>
		</>
	);

	const dialogs = (
		<>
			<ConfirmDialog
				open={dialog === "archive"}
				onOpenChange={(next) => !next && close()}
				title={t("sessionMenu.archiveDialog.title", { title: session.title })}
				description={
					terminal ? t("sessionMenu.archiveDialog.terminal") : t("sessionMenu.archiveDialog.agent")
				}
				action={t("sessionMenu.archiveDialog.action")}
				pending={archive.pending}
				error={archive.error}
				onConfirm={() =>
					archive.run(session, () => {
						close();
						onArchived?.();
					})
				}
			/>
			<ConfirmDialog
				open={dialog === "delete"}
				onOpenChange={(next) => !next && close()}
				title={t("sessionMenu.deleteDialog.title", { title: session.title })}
				description={
					archived || !isBusy(session)
						? t("sessionMenu.deleteDialog.description")
						: t("sessionMenu.deleteDialog.running")
				}
				action={t("common:actions.delete")}
				destructive
				pending={remove.isPending || archiving.isPending}
				error={remove.error?.message ?? archiving.error?.message}
				onConfirm={() => {
					// Only archived sessions are deleted: one still out is archived first
					// (which stops what runs in it).
					const gone = () =>
						remove.mutate(
							{ id: session.id },
							{
								onSuccess: () => {
									close();
									onDeleted?.();
								},
							},
						);
					if (archived) gone();
					else archiving.mutate({ id: session.id }, { onSuccess: gone });
				}}
			/>
			<NameDialog
				open={dialog === "folder"}
				onOpenChange={(next) => !next && close()}
				title={t("folderDialog.newTitle")}
				label={t("folderDialog.name")}
				action={t("folderDialog.createAndMove")}
				pending={createFolder.isPending}
				error={createFolder.error?.message}
				onSubmit={(name) =>
					createFolder.mutate(
						{ project: session.projectId, data: { name } },
						{
							onSuccess: (folder) => {
								close();
								file(folder.id);
							},
						},
					)
				}
			/>
		</>
	);

	return { items, dialogs };
}

/** A session's ⋯ menu (and its row's right-click): {@link useSessionMenu}'s items. */
export function SessionMenu({
	trigger,
	open,
	onOpenChange,
	...options
}: SessionMenuOptions & {
	trigger: React.ReactNode;
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
}) {
	const { items, dialogs } = useSessionMenu(options);
	return (
		<>
			<DropdownMenu open={open} onOpenChange={onOpenChange}>
				<DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
				<DropdownMenuContent align="end" onCloseAutoFocus={(event) => event.preventDefault()}>
					{items}
				</DropdownMenuContent>
			</DropdownMenu>
			{dialogs}
		</>
	);
}
