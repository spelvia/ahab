import { type IDockviewHeaderActionsProps, type IDockviewPanelProps } from "dockview-react";
import { Bot, Plus, SquareTerminal } from "lucide-react";
import { createContext, forwardRef, useContext, useEffect, useMemo } from "react";

import { WindowTab } from "@/app/shell/window-tabs";
import { PluginWindow, useContributions } from "@/modules/plugins";
import {
	keepOpen,
	type ClientClass,
	type OpenTab,
	type Panels,
	readOnlyProjects,
	StartSurface,
	type Tab,
	Tiles,
	type TilesHandle,
	useProjectLayout,
	useProjects,
} from "@/modules/projects";
import { NewSession, SessionView, touchChatTarget, useSession } from "@/modules/sessions";
import type { Session, SessionFolder, Project } from "@/shared/api/generated/ahab.schemas";
import { i18n, useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/** What windows and empty tiles need; Dockview renders them in its own tree. */
export interface WorkbenchContext {
	project?: Project;
	onBrowse: () => void;
	onCommand: () => void;
	/** A session started in window `from` (or in the empty tiles): show it there. */
	onSessionStarted: (session: Session, from?: string) => void;
	/** Open another New session window. */
	onNewSession: () => void;
	/** Start a terminal in the project, in a window of its own. */
	/** None while the Terminal feature is off. */
	onNewTerminal?: () => void;
	/** Open a file of the project, at a line. */
	onOpenFileDiff?: (path: string) => Promise<void>;
	onOpenFile: (path: string, line?: number) => void;
	/** One window at a time: a session's tabs show one at a time too. */
	phone?: boolean;
	/** Whether terminals may be started (the Terminal feature is on). */
	terminals?: boolean;
}

export const Workbench = createContext<WorkbenchContext>({
	onBrowse: () => {},
	onCommand: () => {},
	onSessionStarted: () => {},
	onNewSession: () => {},
	onNewTerminal: undefined,
	onOpenFile: () => {},
});

/** A session's window. */
export function sessionTab(session: Pick<Session, "id" | "title">): Tab {
	return {
		id: `session:${session.id}`,
		component: "session",
		title: session.title,
		params: { sessionId: session.id },
	};
}

/**
 * A window for starting a session, perhaps in a folder, perhaps with context
 * already in its composer (`draftKey`); it becomes the session's
 * window.
 */
export function newSessionTab(folder?: SessionFolder, draftKey?: string): Tab {
	return {
		id: `new-session:${Date.now()}`,
		component: "new-session",
		title: i18n.t("windows.newSession", { ns: "shell" }),
		params: {
			...(folder ? { folderId: folder.id, folderName: folder.name } : {}),
			...(draftKey ? { draftKey } : {}),
		},
		keep: Boolean(draftKey),
	};
}

/**
 * A session's window: its chats and terminals as tabs. Brought
 * forward, a session with a chat becomes where context goes (the
 * context target).
 */
function SessionPanel({ api, containerApi, params }: IDockviewPanelProps<{ sessionId: string }>) {
	const session = useSession(params.sessionId);
	const { project, phone, terminals, onOpenFile, onOpenFileDiff } = useContext(Workbench);
	const chatIn =
		session.data && session.data.chats.length > 0 && !session.data.archivedAt
			? session.data.projectId
			: undefined;
	useEffect(() => {
		if (!chatIn) return;
		if (api.isActive) touchChatTarget(chatIn, params.sessionId);
		const watching = api.onDidActiveChange((event) => {
			if (event.isActive) touchChatTarget(chatIn, params.sessionId);
		});
		return () => watching.dispose();
	}, [api, chatIn, params.sessionId]);
	return (
		<SessionView
			sessionId={params.sessionId}
			project={project}
			phone={phone}
			terminals={terminals}
			onOpenFile={onOpenFile}
			onOpenFileDiff={onOpenFileDiff}
			onGone={() => api.close()}
			onActivity={() => {
				const panel = containerApi.getPanel(api.id);
				if (panel) keepOpen(panel);
			}}
		/>
	);
}

/** A New session window: the composer, in the current project. */
function NewSessionPanel({
	api,
	params,
}: IDockviewPanelProps<{ folderId?: string; folderName?: string; draftKey?: string }>) {
	const { project, onSessionStarted, onOpenFile } = useContext(Workbench);
	if (!project) return null;
	return (
		<div className="flex h-full items-start justify-center overflow-auto p-4 pt-[10vh] md:p-8 md:pt-[15vh]">
			<NewSession
				project={project}
				folder={
					params.folderId ? { id: params.folderId, name: params.folderName ?? "" } : undefined
				}
				draftKey={params.draftKey ?? `new:${api.id}`}
				onOpenFile={onOpenFile}
				onStarted={(started) => onSessionStarted(started, api.id)}
			/>
		</div>
	);
}

/** A plugin's window: its component, from the plugin's web entry. */
function PluginPanel({
	api,
	params,
}: IDockviewPanelProps<{ plugin: string; kind: string; params?: unknown }>) {
	return (
		<PluginWindow
			plugin={params.plugin}
			kind={params.kind}
			params={params.params}
			onClose={() => api.close()}
		/>
	);
}

const panels: Panels = {
	session: SessionPanel as Panels[string],
	"new-session": NewSessionPanel as Panels[string],
	plugin: PluginPanel as Panels[string],
};

/**
 * Kinds of window that are gone from the tiles: Problems lives in the
 * bottom panel now. Saved layouts holding them load without them.
 */
const RETIRED = ["problems"] as const;

/** No windows open: start a session right here. */
function EmptyProject() {
	const { project, onBrowse, onCommand, onSessionStarted } = useContext(Workbench);
	if (!project) return <StartSurface onBrowse={onBrowse} onCommand={onCommand} />;
	return (
		<div className="flex h-full items-start justify-center overflow-auto p-4 pt-[10vh] md:p-8 md:pt-[15vh]">
			<NewSession project={project} onStarted={(started) => onSessionStarted(started)} />
		</div>
	);
}

/** The + at the end of each tab bar: a new agent session or terminal. */
function AddWindow(_props: IDockviewHeaderActionsProps) {
	const { onNewSession, onNewTerminal } = useContext(Workbench);
	const { newWindow } = useContributions();
	const { t } = useTranslation("shell");
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				aria-label={t("windows.add.label")}
				title={t("windows.add.title")}
				className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring mx-1 my-auto grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2"
			>
				<Plus className="size-4" />
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuItem onSelect={onNewSession}>
					<Bot />
					{t("windows.add.agent")}
				</DropdownMenuItem>
				{onNewTerminal ? (
					<DropdownMenuItem onSelect={onNewTerminal}>
						<SquareTerminal />
						{t("windows.add.terminal")}
					</DropdownMenuItem>
				) : null}
				{newWindow.length ? <DropdownMenuSeparator /> : null}
				{newWindow.map((item) => (
					<DropdownMenuItem key={item.key} onSelect={item.run}>
						{item.icon}
						{item.title}
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

/**
 * A project's windows: sessions, files and new sessions, tiled and
 * tabbed as last arranged (kept per kind of screen).
 */
export const ProjectTiles = forwardRef<
	TilesHandle,
	{
		project: Project;
		kind: ClientClass;
		onActive: (id: string | undefined, params: Record<string, unknown> | undefined) => void;
		onTabs?: (tabs: OpenTab[]) => void;
		/** Phones list the windows in a sheet instead. */
		hideTabs?: boolean;
		/** A window to open once ready, then `onOpened`. */
		openOnReady?: Tab;
		onOpened?: () => void;
		/** A saved layout held a Problems window: show the panel instead. */
		onProblemsRetired?: () => void;
	}
>(function ProjectTiles(
	{ project, kind, onActive, onTabs, hideTabs, openOnReady, onOpened, onProblemsRetired },
	ref,
) {
	const store = useProjectLayout(project.id, kind);
	// File tabs may hold other projects' files: each is as its own project allows.
	const projects = useProjects().data?.items;
	const readOnly = useMemo(() => readOnlyProjects(project, projects), [project, projects]);
	return (
		<Tiles
			key={store.key}
			ref={ref}
			store={store}
			currentProject={project.id}
			empty={EmptyProject}
			panels={panels}
			headerActions={AddWindow}
			tabComponent={WindowTab}
			onActive={onActive}
			onTabs={onTabs}
			hideTabs={hideTabs}
			openOnReady={openOnReady}
			onOpened={onOpened}
			readOnlyProjects={readOnly}
			retired={RETIRED}
			onRetired={(kinds) => {
				if (kinds.includes("problems")) onProblemsRetired?.();
			}}
		/>
	);
});
