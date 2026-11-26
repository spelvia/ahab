import { cn } from "cn";
import {
	Archive,
	Bot,
	ChevronRight,
	Folder as FolderIcon,
	MoreHorizontal,
	Pencil,
	Pin,
	SquareTerminal,
	Trash2,
} from "lucide-react";
import { useState } from "react";

import { AgentIcon, agentLabel } from "@/modules/machines";
import {
	useCreateFolder,
	useDeleteFolder,
	useFolders,
	useRenameFolder,
	useSessions,
	useUpdateSession,
} from "@/modules/sessions/session/api";
import { PartDialogs, PartMenuItems } from "@/modules/sessions/session/components/part-tabs";
import {
	ListOptions,
	SelectionBar,
} from "@/modules/sessions/session/components/session-list-tools";
import { SessionMenu } from "@/modules/sessions/session/components/session-menu";
import {
	SessionSearchField,
	SessionSearchResults,
} from "@/modules/sessions/session/components/session-search";
import { TitleField } from "@/modules/sessions/session/components/title-field";
import { showPart, useArchive } from "@/modules/sessions/session/hooks";
import {
	type FolderNode,
	selectRange,
	sessionGroups,
	sessionTree,
	folderWithin,
	isBusy,
	type Part,
	partsOf,
	partState,
	partTitle,
} from "@/modules/sessions/session/model";
import { shownTitle } from "@/modules/sessions/terminal";
import type { Session, SessionFolder } from "@/shared/api/generated/ahab.schemas";
import { ago } from "@/shared/format";
import { i18n, useTranslation } from "@/shared/i18n";
import {
	closedFolders,
	openSessions,
	rememberFolderClosed,
	rememberSessionListView,
	rememberSessionOpen,
	sessionListView,
} from "@/shared/preferences/store";
import {
	ContextMenu,
	ContextMenuTrigger,
	ContextMenuItem,
	ContextMenuContent,
	ContextMenuSeparator,
} from "@/shared/ui/context-menu";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";
import { toastError } from "@/shared/ui/toast";

/** The drag payload's type: a session's ID. */
const DRAGGED = "application/x-ahab-session";

/** A session's state in a dot and a word: a summary of its chats and terminals. */
export function sessionState(session: Session): { dot: string; label: string; waiting: boolean } {
	if (session.archivedAt)
		return {
			dot: "border border-muted-foreground/40",
			label: i18n.t("sessions:state.archived"),
			waiting: false,
		};
	// A session that is one terminal says what the terminal does.
	const parts = partsOf(session);
	if (parts.length === 1 && parts[0]?.kind === "terminal") return partState(parts[0]);
	// Only terminals run: "running", as a terminal says.
	if (
		session.state === "idle" &&
		!session.chats.some((chat) => chat.state === "live") &&
		session.terminals.some((terminal) => terminal.state === "running")
	)
		return {
			dot: "border border-success",
			label: i18n.t("sessions:state.running"),
			waiting: false,
		};
	switch (session.state) {
		case "needsYou":
			return { dot: "bg-warning", label: i18n.t("sessions:state.needsYou"), waiting: true };
		case "working":
			return {
				dot: "bg-success animate-pulse",
				label: i18n.t("sessions:state.working"),
				waiting: false,
			};
		case "idle":
			return { dot: "border border-success", label: i18n.t("sessions:state.idle"), waiting: false };
		case "ended":
			return {
				dot: "border border-muted-foreground/60",
				label: i18n.t("sessions:state.ended"),
				waiting: false,
			};
	}
}

/**
 * A project's sessions, agents and terminals together: pinned first, then
 * its folders, then the rest by recent activity; archived ones collapsed at the end. Sessions are
 * dragged onto a folder to file them, or onto the list to take them out.
 */
export function SessionList({
	project,
	selected,
	onOpen,
	onGone,
	onNewIn,
	terminals = true,
}: {
	project: string;
	selected?: string;
	onOpen: (session: Session, keep?: boolean) => void;
	/** A session was archived or deleted: its window closes. */
	onGone?: (session: Session) => void;
	/** Start an agent or a terminal filed in a folder. */
	onNewIn?: (kind: "agent" | "terminal", folder?: SessionFolder) => void;
	/** Whether terminals may be started (the Terminal feature is on). */
	terminals?: boolean;
}) {
	const { t } = useTranslation(["sessions", "common"]);
	const sessions = useSessions(project);
	const folders = useFolders(project);
	const update = useUpdateSession();
	const [closed, setClosed] = useState(() => new Set(closedFolders()));
	const [showArchived, setShowArchived] = useState(false);
	const [over, setOver] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	// Sorted and grouped as chosen on this device.
	const [view, setView] = useState(sessionListView);
	// Sessions selected with ⌘/Ctrl or Shift, for what is done to them all.
	const [picked, setPicked] = useState<ReadonlySet<string>>(() => new Set());
	const [anchor, setAnchor] = useState<string>();

	const mine = sessions.data?.items ?? [];
	const current = mine.filter((session) => !session.archivedAt);
	const archived = mine.filter((session) => session.archivedAt);
	const tree = sessionTree(current, folders.data?.items ?? [], view.sort);
	const pinned = tree.sessions.filter((item) => item.pinnedAt);
	const loose = tree.sessions.filter((item) => !item.pinnedAt);
	const create = useCreateFolder();
	const updateFolder = useRenameFolder();
	const [parent, setParent] = useState<string | null | undefined>();
	const toggle = (id: string) => {
		const shut = !closed.has(id);
		rememberFolderClosed(id, shut);
		setClosed((set) => {
			const next = new Set(set);
			if (shut) next.add(id);
			else next.delete(id);
			return next;
		});
	};
	const drop = (event: React.DragEvent, folderId: string | null) => {
		const from = event.dataTransfer.getData("application/x-ahab-folder");
		event.stopPropagation();
		if (from) {
			event.preventDefault();
			setOver(null);
			if (!folderId || !folderWithin(folders.data?.items ?? [], folderId, from))
				updateFolder.mutate({ id: from, data: { parentId: folderId } }, { onError: toastError });
			return;
		}
		const id = event.dataTransfer.getData(DRAGGED);
		setOver(null);
		const session = mine.find((item) => item.id === id);
		if (!session || (session.folderId ?? null) === folderId) return;
		event.preventDefault();
		event.stopPropagation();
		update.mutate({ id, data: { folderId } }, { onError: toastError });
	};
	const target = (key: string, folderId: string | null) => ({
		onDragOver: (event: React.DragEvent) => {
			if (
				!event.dataTransfer.types.some(
					(type) => type === DRAGGED || type === "application/x-ahab-folder",
				)
			)
				return;
			event.preventDefault();
			event.stopPropagation();
			setOver(key);
		},
		onDragLeave: () => setOver((now) => (now === key ? null : now)),
		onDrop: (event: React.DragEvent) => drop(event, folderId),
	});

	if (sessions.isPending)
		return <p className="text-muted-foreground px-3 text-sm">{t("common:state.loading")}</p>;
	const all = sessions.data?.items ?? [];
	// Searching needs something to search; the field shows once there is.
	const field = all.length > 0 ? <SessionSearchField query={query} onQuery={setQuery} /> : null;
	// The sessions as listed, top to bottom, for Shift's ranges and the arrow keys.
	const listed: string[] = [];
	const pick = (id: string, event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => {
		if (event.shiftKey) {
			const range = selectRange(listed, anchor ?? selected ?? id, id);
			setPicked((now) => new Set([...(event.metaKey || event.ctrlKey ? now : []), ...range]));
			return true;
		}
		if (event.metaKey || event.ctrlKey) {
			setPicked((now) => {
				const next = new Set(now);
				// The open one joins the first time, as in a file manager.
				if (next.size === 0 && selected && selected !== id) next.add(selected);
				if (next.has(id)) next.delete(id);
				else next.add(id);
				return next;
			});
			setAnchor(id);
			return true;
		}
		setPicked(new Set());
		setAnchor(id);
		return false;
	};
	const row = (session: Session) => {
		listed.push(session.id);
		return (
			<SessionRow
				key={session.id}
				session={session}
				selected={selected === session.id}
				picked={picked.has(session.id)}
				onPick={(event) => pick(session.id, event)}
				onOpen={onOpen}
				onGone={onGone}
				terminals={terminals}
			/>
		);
	};
	const pickedSessions = mine.filter((session) => picked.has(session.id));
	// Up and down move between rows; with Shift they select as they go.
	const onKeyDown = (event: React.KeyboardEvent) => {
		if (event.key === "Escape" && picked.size) {
			event.stopPropagation();
			setPicked(new Set());
			return;
		}
		if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
		const from = (event.target as HTMLElement).closest<HTMLElement>("[data-session-row]")?.dataset
			.sessionRow;
		if (!from) return;
		const at = listed.indexOf(from);
		const to = listed[at + (event.key === "ArrowDown" ? 1 : -1)];
		if (!to) return;
		event.preventDefault();
		event.currentTarget
			.querySelector<HTMLElement>(`[data-session-row="${CSS.escape(to)}"]`)
			?.focus();
		if (event.shiftKey) {
			setPicked((now) => new Set([...(now.size ? now : [from]), to]));
			setAnchor((now) => now ?? from);
		}
	};
	const grouped =
		view.group === "folders"
			? undefined
			: sessionGroups(
					current.filter((session) => !session.pinnedAt),
					view.group,
					view.sort,
					(grouping, key) => {
						if (grouping === "status") return t(`state.${key as "idle"}`);
						if (grouping === "day") return t(`sessionList.view.days.${key as "today"}`);
						if (key === "terminal") return t("sessionList.terminal");
						if (key === "none") return t("sessionList.view.noAgent");
						return agentLabel(key);
					},
				);

	const group = ({
		folder,
		count,
		folders: inner,
		sessions: items,
		waiting,
	}: FolderNode): React.ReactNode => (
		<FolderGroup
			key={folder.id}
			folder={folder}
			open={!closed.has(folder.id)}
			onToggle={() => toggle(folder.id)}
			highlighted={over === folder.id}
			dropTarget={target(folder.id, folder.id)}
			count={count}
			waiting={waiting}
			empty={inner.length + items.length === 0}
			onFolder={() => setParent(folder.id)}
			onNewIn={onNewIn}
			terminals={terminals}
		>
			<ul>{items.filter((item) => item.pinnedAt).map(row)}</ul>
			{inner.map(group)}
			<ul>{items.filter((item) => !item.pinnedAt).map(row)}</ul>
		</FolderGroup>
	);

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				<div
					{...target("root", null)}
					onKeyDown={onKeyDown}
					className={cn("flex-1", over === "root" && "bg-accent/30")}
				>
					{field ? (
						<div className="flex items-start pr-2 [&>:first-child]:min-w-0 [&>:first-child]:flex-1 [&>:first-child]:pr-1">
							{field}
							<ListOptions
								sort={view.sort}
								group={view.group}
								onChange={(next) => {
									setView(next);
									rememberSessionListView(next);
								}}
							/>
						</div>
					) : null}
					{pickedSessions.length ? (
						<SelectionBar
							sessions={pickedSessions}
							folders={folders.data?.items ?? []}
							onClear={() => setPicked(new Set())}
							onGone={onGone}
						/>
					) : null}
					{query.trim() ? (
						<SessionSearchResults
							project={project}
							query={query}
							sessions={all}
							selected={selected}
							onOpen={onOpen}
						/>
					) : (
						<>
							{" "}
							{!mine.length && !tree.folders.length && (
								<p className="text-muted-foreground px-3 py-1 text-sm">{t("sessionList.empty")}</p>
							)}
							{pinned.length > 0 ? (
								<ul
									aria-label={t("sessionList.pinned")}
									className="border-border mb-1 border-b pb-1"
								>
									{pinned.map(row)}
								</ul>
							) : null}
							{grouped ? (
								grouped.map((item) => (
									<section key={item.key} aria-label={item.label || t("sessionList.sessions")}>
										{item.label ? (
											<p className="text-muted-foreground px-3 pt-2 pb-0.5 text-[0.6875rem] font-medium tracking-wide uppercase">
												{item.label}
											</p>
										) : null}
										<ul>{item.sessions.map(row)}</ul>
									</section>
								))
							) : (
								<>
									{tree.folders.map(group)}
									<ul aria-label={t("sessionList.sessions")}>{loose.map(row)}</ul>
								</>
							)}
							{archived.length > 0 ? (
								<div className="mt-2">
									<button
										type="button"
										aria-expanded={showArchived}
										onClick={() => setShowArchived((open) => !open)}
										className="text-muted-foreground hover:text-foreground flex w-full items-center gap-1.5 px-3 py-1.5 text-xs"
									>
										<ChevronRight
											className={cn("size-3.5 transition-transform", showArchived && "rotate-90")}
										/>
										<Archive className="size-3.5" />
										{t("sessionList.showArchived", { count: archived.length })}
									</button>
									{showArchived ? (
										<ul aria-label={t("sessionList.archived")}>{archived.map(row)}</ul>
									) : null}
								</div>
							) : null}
						</>
					)}
				</div>
			</ContextMenuTrigger>
			<ContextMenuContent>
				{onNewIn && (
					<ContextMenuItem onSelect={() => onNewIn("agent")}>
						<Bot />
						{t("addMenu.newAgent")}
					</ContextMenuItem>
				)}
				{onNewIn && terminals && (
					<ContextMenuItem onSelect={() => onNewIn("terminal")}>
						<SquareTerminal />
						{t("addMenu.newTerminal")}
					</ContextMenuItem>
				)}
				<ContextMenuItem onSelect={() => setParent(null)}>
					<FolderIcon />
					{t("addMenu.newFolder")}
				</ContextMenuItem>
			</ContextMenuContent>
			<NameDialog
				open={parent !== undefined}
				onOpenChange={(open) => {
					if (!open) setParent(undefined);
				}}
				title={t("folderDialog.newTitle")}
				label={t("folderDialog.name")}
				action={t("folderDialog.create")}
				pending={create.isPending}
				error={create.error?.message}
				onSubmit={(name) =>
					create.mutate(
						{ project, data: { name, parentId: parent } },
						{
							onSuccess: () => {
								if (parent && closed.has(parent)) toggle(parent);
								setParent(undefined);
							},
						},
					)
				}
			/>
		</ContextMenu>
	);
}

/** One of a session's chats or terminals, under it in the tree. */
function PartRow({ session, part, onOpen }: { session: Session; part: Part; onOpen: () => void }) {
	const { t } = useTranslation("sessions");
	const [dialog, setDialog] = useState<"rename" | "remove" | "move" | null>(null);
	const state = partState(part);
	const title = partTitle(part);
	return (
		<li className="group/part relative">
			<button
				type="button"
				onClick={onOpen}
				className="hover:bg-accent/60 focus-visible:ring-ring flex min-h-9 w-full items-center gap-2 py-1 pr-10 pl-9 text-left text-[0.8125rem] outline-none focus-visible:ring-1 focus-visible:ring-inset md:min-h-7"
			>
				{part.kind === "chat" ? (
					<AgentIcon agent={part.chat.agent} className="size-3.5 shrink-0" />
				) : (
					<SquareTerminal className="text-muted-foreground size-3.5 shrink-0" aria-hidden />
				)}
				<span className="min-w-0 flex-1 truncate">{title}</span>
				<span className={cn("size-1.5 shrink-0 rounded-full", state.dot)} aria-hidden />
				<span
					className={cn(
						"text-muted-foreground shrink-0 text-[0.6875rem]",
						state.waiting && "text-warning",
					)}
				>
					{state.label}
				</span>
			</button>
			<DropdownMenu>
				<DropdownMenuTrigger
					aria-label={t("sessionList.actions", { title })}
					className="text-muted-foreground hover:text-foreground hover:bg-accent absolute inset-y-0 right-1.5 my-auto grid size-7 place-items-center rounded-md outline-none md:size-6 md:opacity-0 md:group-hover/part:opacity-100 md:focus-visible:opacity-100"
				>
					<MoreHorizontal className="size-4" />
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<PartMenuItems
						session={session}
						part={part}
						onDialog={setDialog}
						Item={DropdownMenuItem}
						Separator={DropdownMenuSeparator}
					/>
				</DropdownMenuContent>
			</DropdownMenu>
			<PartDialogs session={session} part={part} dialog={dialog} onDone={() => setDialog(null)} />
		</li>
	);
}

function FolderGroup({
	folder,
	open,
	onToggle,
	onFolder,
	highlighted,
	dropTarget,
	count,
	waiting,
	empty,
	onNewIn,
	terminals,
	children,
}: {
	folder: SessionFolder;
	onNewIn?: (kind: "agent" | "terminal", folder?: SessionFolder) => void;
	terminals: boolean;
	onFolder: () => void;
	open: boolean;
	onToggle: () => void;
	highlighted: boolean;
	dropTarget: React.HTMLAttributes<HTMLElement>;
	count: number;
	waiting: number;
	empty: boolean;
	children: React.ReactNode;
}) {
	const { t } = useTranslation("sessions");
	const rename = useRenameFolder();
	const remove = useDeleteFolder();
	const [renaming, setRenaming] = useState(false);
	const items = (
		Item: typeof DropdownMenuItem | typeof ContextMenuItem,
		Separator: typeof ContextMenuSeparator,
	) => (
		<>
			{" "}
			{onNewIn ? (
				<>
					<Item onSelect={() => onNewIn("agent", folder)}>
						<Bot />
						{t("sessionList.newAgentHere")}
					</Item>
					{terminals ? (
						<Item onSelect={() => onNewIn("terminal", folder)}>
							<SquareTerminal />
							{t("sessionList.newTerminalHere")}
						</Item>
					) : null}
					<Separator />
				</>
			) : null}
			<Item onSelect={onFolder}>
				<FolderIcon />
				{t("addMenu.newFolder")}
			</Item>
			<Item
				onSelect={() =>
					rename.mutate(
						{ id: folder.id, data: { pinned: !folder.pinnedAt } },
						{ onError: toastError },
					)
				}
			>
				<Pin />
				{t(folder.pinnedAt ? "sessionMenu.unpin" : "sessionMenu.pin")}
			</Item>
			<Item onSelect={() => setRenaming(true)}>
				<Pencil />
				{t("sessionList.renameFolder")}
			</Item>
			<Item
				variant="destructive"
				onSelect={() => remove.mutate({ id: folder.id }, { onError: toastError })}
			>
				<Trash2 />
				{t("sessionList.deleteFolder")}
			</Item>
		</>
	);
	return (
		<section
			aria-label={t("sessionList.folder", { name: folder.name })}
			{...dropTarget}
			className={cn("rounded-md", highlighted && "bg-accent/50 ring-ring/40 ring-1 ring-inset")}
		>
			<ContextMenu>
				<ContextMenuTrigger asChild>
					<div
						draggable
						onDragStart={(e) => {
							e.stopPropagation();
							e.dataTransfer.setData("application/x-ahab-folder", folder.id);
						}}
						className="group flex items-center pr-1.5"
					>
						<button
							type="button"
							aria-expanded={open}
							onClick={onToggle}
							className="text-foreground hover:bg-accent/60 flex min-h-9 min-w-0 flex-1 items-center gap-1.5 px-3 text-left text-[0.8125rem] font-medium md:min-h-8"
						>
							<ChevronRight
								className={cn(
									"text-muted-foreground size-3.5 shrink-0 transition-transform",
									open && "rotate-90",
								)}
							/>
							<FolderIcon className="text-muted-foreground size-3.5 shrink-0" />
							<span className="truncate">{folder.name}</span>
							{!open && count > 0 && (
								<span aria-hidden className="text-muted-foreground ml-auto text-xs tabular-nums">
									{count}
								</span>
							)}
							{folder.pinnedAt && <Pin className="text-muted-foreground size-3" aria-hidden />}
							{!open && waiting > 0 ? (
								<span className="bg-warning ml-1 size-1.5 shrink-0 rounded-full" />
							) : null}
						</button>
						<DropdownMenu>
							<DropdownMenuTrigger
								aria-label={t("sessionList.folderActions", { name: folder.name })}
								className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-7 place-items-center rounded-md outline-none group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100 md:opacity-0"
							>
								<MoreHorizontal className="size-4" />
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end">
								{items(DropdownMenuItem, DropdownMenuSeparator)}
							</DropdownMenuContent>
						</DropdownMenu>
					</div>
				</ContextMenuTrigger>
				<ContextMenuContent>{items(ContextMenuItem, ContextMenuSeparator)}</ContextMenuContent>
			</ContextMenu>
			{open && empty ? (
				<p className="text-muted-foreground py-1 pr-3 pl-10 text-xs">
					{t("sessionList.emptyFolder")}
				</p>
			) : null}
			{open && !empty ? (
				<div className="border-border/70 ml-3 border-l pl-1">{children}</div>
			) : null}
			<NameDialog
				open={renaming}
				onOpenChange={setRenaming}
				title={t("sessionList.renameFolderTitle")}
				label={t("folderDialog.name")}
				initial={folder.name}
				action={t("sessionList.rename")}
				pending={rename.isPending}
				error={rename.error?.message}
				onSubmit={(name) =>
					rename.mutate({ id: folder.id, data: { name } }, { onSuccess: () => setRenaming(false) })
				}
			/>
		</section>
	);
}

function SessionRow({
	session,
	selected,
	picked,
	onPick,
	onOpen,
	onGone,
	terminals,
}: {
	session: Session;
	selected: boolean;
	/** Chosen, with others, for what is done to them all. */
	picked?: boolean;
	/** A click with ⌘/Ctrl or Shift selects instead of opening: true when it did. */
	onPick?: (event: React.MouseEvent) => boolean;
	onOpen: (session: Session, keep?: boolean) => void;
	onGone?: (session: Session) => void;
	terminals: boolean;
}) {
	const { t } = useTranslation("sessions");
	const gone = () => onGone?.(session);
	const [renaming, setRenaming] = useState(false);
	const [menu, setMenu] = useState(false);
	const archive = useArchive();
	const state = sessionState(session);
	const archived = Boolean(session.archivedAt);
	// A session holding several chats and terminals opens in the tree.
	const parts = partsOf(session);
	const several = parts.length > 1;
	const [open, setOpen] = useState(() => openSessions().includes(session.id));
	const toggle = () => {
		rememberSessionOpen(session.id, !open);
		setOpen(!open);
	};
	// Its oldest chat's agent stands for it in the row.
	const agent = session.chats.at(-1)?.agent;

	const body = (
		<>
			<span className={cn("size-2 shrink-0 rounded-full", state.dot)} aria-hidden />
			<span className="min-w-0 flex-1">
				{renaming ? (
					<TitleField session={session} onDone={() => setRenaming(false)} className="w-full" />
				) : (
					<span className="flex items-center gap-1 text-[0.8125rem]">
						<span className="truncate">{shownTitle(session)}</span>
						{session.pinnedAt ? (
							<Pin
								className="text-muted-foreground size-3 shrink-0"
								aria-label={t("sessionList.pinnedIcon")}
							/>
						) : null}
					</span>
				)}
				<span className="text-muted-foreground flex items-center gap-1.5 text-[0.6875rem]">
					{session.kind === "terminal" ? (
						<>
							<SquareTerminal className="size-3.5" aria-hidden />
							<span className="truncate">{t("sessionList.terminal")}</span>·
						</>
					) : null}
					{agent ? <AgentIcon agent={agent} className="size-3.5" /> : null}
					{agent ? <span className="truncate">{agentLabel(agent)}</span> : null}
					{several ? (
						<span className="shrink-0">{t("sessionList.more", { count: parts.length - 1 })}</span>
					) : null}
					{agent || several ? "·" : null}
					<span className={cn("shrink-0", state.waiting && "text-warning font-medium")}>
						{state.label}
					</span>
				</span>
			</span>
			<span
				className={cn(
					// The row's actions show on hover and on focus: the time makes way
					// for them in both, and keeps their room so the title stops short.
					"min-w-6 shrink-0 text-right text-[0.6875rem] md:group-focus-within:invisible md:group-hover:invisible",
					menu && "md:invisible",
					"text-muted-foreground",
				)}
			>
				{ago(session.updatedAt)}
			</span>
		</>
	);
	const rowClass = cn(
		"flex min-h-11 w-full items-center gap-2.5 py-1.5 pr-10 pl-6 text-left outline-none md:min-h-9",
		picked
			? "bg-primary/10 text-heading ring-primary/40 ring-1 ring-inset"
			: selected
				? "bg-accent text-heading"
				: "hover:bg-accent/60",
		(session.state === "ended" || archived) && !selected && "text-muted-foreground",
	);

	return (
		<li
			className="group relative"
			draggable={!renaming && !archived}
			onDragStart={(event) => {
				event.dataTransfer.setData(DRAGGED, session.id);
				event.dataTransfer.effectAllowed = "move";
			}}
			onContextMenu={(event) => {
				event.preventDefault();
				event.stopPropagation();
				setMenu(true);
			}}
		>
			{renaming ? (
				<div className={rowClass}>{body}</div>
			) : (
				<button
					type="button"
					data-session-row={session.id}
					onClick={(event) => {
						if (onPick?.(event)) return;
						onOpen(session);
					}}
					onDoubleClick={() => onOpen(session, true)}
					aria-current={selected ? "true" : undefined}
					aria-selected={picked || undefined}
					className={cn(
						rowClass,
						"focus-visible:ring-ring focus-visible:ring-1 focus-visible:ring-inset",
					)}
				>
					{body}
				</button>
			)}
			{renaming ? null : (
				<div
					className={cn(
						"absolute inset-y-0 right-1.5 flex items-center gap-0.5",
						"md:opacity-0 md:group-focus-within:opacity-100 md:group-hover:opacity-100",
						menu && "md:opacity-100",
					)}
				>
					{/* Busy sessions are archived from the menu, which asks first. */}
					{!archived && !isBusy(session) ? (
						<button
							type="button"
							aria-label={t("sessionList.archive", { title: session.title })}
							title={t("sessionList.archiveTitle")}
							onClick={() => archive.run(session, gone)}
							className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-7 place-items-center rounded-md outline-none max-md:hidden"
						>
							<Archive className="size-3.5" />
						</button>
					) : null}
					<SessionMenu
						session={session}
						open={menu}
						onOpenChange={setMenu}
						onRename={() => setRenaming(true)}
						onArchived={gone}
						onDeleted={gone}
						onOpen={() => onOpen(session, true)}
						terminals={terminals}
						trigger={
							<button
								type="button"
								aria-label={t("sessionList.actions", { title: session.title })}
								className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-8 place-items-center rounded-md outline-none md:size-7"
							>
								<MoreHorizontal className="size-4" />
							</button>
						}
					/>
				</div>
			)}
			{several ? (
				<button
					type="button"
					aria-expanded={open}
					aria-label={
						open
							? t("sessionView.collapse", { title: session.title })
							: t("sessionView.expand", { title: session.title })
					}
					onClick={toggle}
					className="text-muted-foreground hover:text-foreground absolute top-2.5 left-1 grid size-5 place-items-center rounded md:top-2"
				>
					<ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
				</button>
			) : null}
			{several && open ? (
				<ul aria-label={t("sessionView.tabs")} className="pb-1">
					{parts.map((part) => (
						<PartRow
							key={part.key}
							session={session}
							part={part}
							onOpen={() => {
								showPart(session.id, part.key);
								onOpen(session, true);
							}}
						/>
					))}
				</ul>
			) : null}
		</li>
	);
}
