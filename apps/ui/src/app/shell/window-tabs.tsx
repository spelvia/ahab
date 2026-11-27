import { cn } from "cn";
import { DockviewDefaultTab, type IDockviewPanelHeaderProps } from "dockview-react";
import {
	ArrowRightToLine,
	Columns2,
	Copy,
	CopyX,
	FolderSearch,
	ListX,
	MessageSquarePlus,
	MessagesSquare,
	Rows2,
	X,
} from "lucide-react";
import { useContext, useEffect, useState } from "react";

import { absolutePath, fileTooltip, sessionTooltip } from "@/app/shell/tab-tooltip";
import { Workbench } from "@/app/shell/workbench-tiles";
import { isPreview, keepOpen, useUnsaved } from "@/modules/projects";
import {
	SessionPartPicker,
	sessionState,
	TitleField,
	useDraft,
	useSession,
	useSessionMenu,
	shownTitle,
} from "@/modules/sessions";
import type { Session, Project } from "@/shared/api/generated/ahab.schemas";
import { useChatContext } from "@/shared/chat-context/context";
import { fileReference } from "@/shared/chat-context/model";
import { externalName } from "@/shared/editor/registry";
import { i18n, useTranslation } from "@/shared/i18n";
import { copyText } from "@/shared/ui/clipboard";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { FileIcon } from "@/shared/ui/file-icon";
import { toast } from "@/shared/ui/toast";

/** The actions every window's tab offers, whatever the window holds. */
function WindowItems({ tab }: { tab: IDockviewPanelHeaderProps }) {
	const { t } = useTranslation("shell");
	const { api } = tab;
	const siblings = api.group.panels;
	const index = siblings.findIndex((panel) => panel.id === api.id);
	const alone = siblings.length < 2;
	const closeAll = (panels: typeof siblings) => {
		// Closing changes the group's list, so close from a copy.
		for (const panel of Array.from(panels)) panel.api.close();
	};
	return (
		<>
			{isPreview(tab.params) ? (
				<DropdownMenuItem onSelect={() => keepOpen(tab)}>{t("tab.keepOpen")}</DropdownMenuItem>
			) : null}
			<DropdownMenuItem onSelect={() => api.close()}>
				<X />
				{t("actions.close", { ns: "common" })}
			</DropdownMenuItem>
			<DropdownMenuItem
				disabled={alone}
				onSelect={() => closeAll(siblings.filter((panel) => panel.id !== api.id))}
			>
				<ListX />
				{t("tab.closeOthers")}
			</DropdownMenuItem>
			<DropdownMenuItem
				disabled={index === siblings.length - 1}
				onSelect={() => closeAll(siblings.slice(index + 1))}
			>
				<ArrowRightToLine />
				{t("tab.closeToRight")}
			</DropdownMenuItem>
			<DropdownMenuItem onSelect={() => closeAll(siblings)}>
				<CopyX />
				{t("tab.closeAll")}
			</DropdownMenuItem>
			<DropdownMenuSeparator />
			<DropdownMenuItem
				disabled={alone}
				onSelect={() => api.moveTo({ group: api.group, position: "right" })}
			>
				<Columns2 />
				{t("tab.moveRight")}
			</DropdownMenuItem>
			<DropdownMenuItem
				disabled={alone}
				onSelect={() => api.moveTo({ group: api.group, position: "bottom" })}
			>
				<Rows2 />
				{t("tab.moveDown")}
			</DropdownMenuItem>
		</>
	);
}

/** The project a window belongs to, when it is the one open. */
function useOwner(projectId: unknown): Project | undefined {
	const { project } = useContext(Workbench);
	if (typeof projectId !== "string") return undefined;
	return project?.id === projectId ? project : undefined;
}

/**
 * A tab, whatever its window: Dockview's tab (or `title` in its place),
 * after an optional `lead` such as a state's dot, with one right-click menu:
 * `items` for its kind first, then the window's own actions. Hovering it
 * shows `tooltip` (its title by default).
 */
function TabFrame({
	tab,
	lead,
	title,
	items,
	dialogs,
	trail,
	tooltip,
}: {
	tab: IDockviewPanelHeaderProps;
	lead?: React.ReactNode;
	title?: React.ReactNode;
	items?: React.ReactNode;
	dialogs?: React.ReactNode;
	/** After the title: a mark such as unsaved changes. */
	trail?: React.ReactNode;
	tooltip?: string;
}) {
	const [menu, setMenu] = useState(false);
	return (
		<div
			title={tooltip ?? tab.api.title}
			className={cn("relative flex h-full min-w-0 items-center", isPreview(tab.params) && "italic")}
			onDoubleClick={() => keepOpen(tab)}
			onContextMenu={(event) => {
				event.preventDefault();
				setMenu(true);
			}}
		>
			{lead}
			{title ?? <DockviewDefaultTab {...tab} />}
			{trail}
			<DropdownMenu open={menu} onOpenChange={setMenu}>
				{/* Opened by a right-click only; the menu hangs from the tab. */}
				<DropdownMenuTrigger asChild>
					<span aria-hidden className="pointer-events-none absolute inset-0" />
				</DropdownMenuTrigger>
				<DropdownMenuContent align="start" onCloseAutoFocus={(event) => event.preventDefault()}>
					{items ? (
						<>
							{items}
							<DropdownMenuSeparator />
						</>
					) : null}
					<WindowItems tab={tab} />
				</DropdownMenuContent>
			</DropdownMenu>
			{dialogs}
		</div>
	);
}

/**
 * A session's tab: its state's dot before the title, renamed in place, and
 * the session's actions in its menu. Archiving or deleting it closes the
 * window; renamed elsewhere, the tab follows.
 */
function SessionTab({ tab, session }: { tab: IDockviewPanelHeaderProps; session: Session }) {
	const { t } = useTranslation("shell");
	const { api } = tab;
	const owner = useOwner(session.projectId);
	const { terminals } = useContext(Workbench);
	const [renaming, setRenaming] = useState(false);
	const { items, dialogs } = useSessionMenu({
		session,
		onRename: () => setRenaming(true),
		onArchived: () => api.close(),
		onDeleted: () => api.close(),
		// Already its window: New chat here and New terminal here show in it.
		onOpen: () => api.setActive(),
	});
	const title = shownTitle(session);
	useEffect(() => {
		if (title !== api.title) api.setTitle(title);
	}, [title, api]);
	const state = sessionState(session);
	// Context waiting in its composer: marked, briefly brighter when it grows.
	const held = useDraft(session.id).length;
	const [grew, setGrew] = useState(false);
	const [seen, setSeen] = useState(held);
	if (held !== seen) {
		setSeen(held);
		if (held > seen) setGrew(true);
	}
	useEffect(() => {
		if (!grew) return;
		const timer = setTimeout(() => setGrew(false), 2000);
		return () => clearTimeout(timer);
	}, [grew]);
	const frame = (
		<TabFrame
			tab={tab}
			tooltip={sessionTooltip(session, owner, title)}
			trail={
				held ? (
					<span
						title={t("tab.context", { count: held })}
						aria-label={t("tab.context", { count: held })}
						className={cn(
							"text-muted-foreground mr-1 flex shrink-0 items-center gap-0.5 rounded px-1 text-[0.6562rem]",
							grew && "bg-primary/20 text-primary animate-pulse",
						)}
					>
						<MessageSquarePlus className="size-3" />
						{held}
					</span>
				) : null
			}
			lead={
				<span
					className={cn("mr-1.5 size-2 shrink-0 rounded-full", state.dot)}
					title={state.label}
					aria-label={state.label}
				/>
			}
			title={
				renaming ? (
					<span className="min-w-0 flex-1 px-2">
						<TitleField session={session} onDone={() => setRenaming(false)} className="w-full" />
					</span>
				) : undefined
			}
			items={items}
			dialogs={dialogs}
		/>
	);
	return renaming ? (
		frame
	) : (
		<SessionPartPicker terminals={terminals} session={session} onActivate={() => api.setActive()}>
			{frame}
		</SessionPartPicker>
	);
}

/** A session's tab keeps its width while loading, renaming or showing extra actions. */
function SessionWindowTab({
	tab,
	sessionId,
}: {
	tab: IDockviewPanelHeaderProps;
	sessionId: string;
}) {
	const session = useSession(sessionId);
	return (
		<div className="ahab-session-tab h-full w-44">
			{session.data ? <SessionTab tab={tab} session={session.data} /> : <TabFrame tab={tab} />}
		</div>
	);
}

function copy(text: string) {
	void copyText(text).then((copied) =>
		toast(i18n.t(copied ? "clipboard.copied" : "clipboard.failed")),
	);
}

/**
 * A file's tab: marked while it has unsaved changes; hovering it shows where
 * the file is on its machine; copy its path, show it among the files, or
 * add the whole file to a chat.
 */
function FileTab({ tab }: { tab: IDockviewPanelHeaderProps }) {
	const { project, onBrowse } = useContext(Workbench);
	const chat = useChatContext();
	const path = String(tab.params.path ?? "");
	const projectId = String(tab.params.projectId ?? "");
	const here = projectId === project?.id;
	const owner = useOwner(projectId);
	const external = path.startsWith("external:");
	const unsaved = useUnsaved(projectId, path);
	const { t } = useTranslation("shell");
	return (
		<TabFrame
			tab={tab}
			tooltip={fileTooltip(path, owner, externalName(path))}
			lead={<FileIcon path={path} className="mr-1.5 size-3.5" />}
			trail={
				unsaved ? (
					<span
						title={t("tab.unsaved")}
						aria-label={t("tab.unsaved")}
						className="bg-warning mr-1 size-2 shrink-0 rounded-full"
					/>
				) : null
			}
			items={
				<>
					{chat && here ? (
						<>
							<DropdownMenuItem onSelect={() => chat.add([fileReference(projectId, path)])}>
								<MessageSquarePlus />
								{chat.target
									? t("tab.addFileTo", { title: chat.target.title })
									: t("tab.addFileToChat")}
							</DropdownMenuItem>
							<DropdownMenuItem onSelect={() => chat.choose([fileReference(projectId, path)])}>
								<MessagesSquare />
								{t("tab.addFileToChat")}
							</DropdownMenuItem>
							<DropdownMenuSeparator />
						</>
					) : null}
					{owner && !external ? (
						<DropdownMenuItem onSelect={() => copy(absolutePath(owner.path, path))}>
							<Copy />
							{t("tab.copyPath")}
						</DropdownMenuItem>
					) : null}
					{external ? null : (
						<DropdownMenuItem onSelect={() => copy(path)}>
							<Copy />
							{t("tab.copyRelativePath")}
						</DropdownMenuItem>
					)}
					<DropdownMenuItem onSelect={onBrowse}>
						<FolderSearch />
						{t("tab.showInFiles")}
					</DropdownMenuItem>
				</>
			}
		/>
	);
}

/** A diff's tab: hovering it shows what it compares and the file's path. */
function DiffTab({ tab }: { tab: IDockviewPanelHeaderProps }) {
	const { t } = useTranslation("shell");
	const owner = useOwner(tab.params.projectId);
	const path = String(tab.params.path ?? "");
	const repository = typeof tab.params.repository === "string" ? tab.params.repository : "";
	const prefix = typeof tab.params.prefix === "string" ? tab.params.prefix : "";
	// The file as a path of the project, as the diff's window finds it.
	const relative = repository
		? `${repository}/${path}`
		: path.startsWith(prefix)
			? path.slice(prefix.length)
			: undefined;
	return (
		<TabFrame
			tab={tab}
			tooltip={t("tabTooltip.diff", {
				title: tab.api.title,
				file: relative === undefined ? path : fileTooltip(relative, owner),
			})}
			lead={<FileIcon path={path} className="mr-1.5 size-3.5" />}
		/>
	);
}

/** Any other window: its title, and the project it starts in. */
function OtherTab({ tab }: { tab: IDockviewPanelHeaderProps }) {
	const { project } = useContext(Workbench);
	const { t } = useTranslation("shell");
	const title = tab.api.title ?? "";
	const tooltip = project
		? t("tabTooltip.otherIn", {
				title,
				project: project.name,
				machine: project.machineName,
			})
		: title;
	return <TabFrame tab={tab} tooltip={tooltip} />;
}

/** Every tab: the same menu, with what its window adds. */
export function WindowTab(props: IDockviewPanelHeaderProps) {
	if (typeof props.params?.sessionId === "string")
		return <SessionWindowTab tab={props} sessionId={props.params.sessionId} />;
	if (props.params?.kind === "file") return <FileTab tab={props} />;
	if (props.params?.kind === "diff") return <DiffTab tab={props} />;
	return <OtherTab tab={props} />;
}
