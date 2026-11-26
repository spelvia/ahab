import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { cn } from "cn";
import { ArrowLeft, ChevronDown, PanelLeft, PanelRight, Search, Settings } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { BottomPanel, usePanel } from "@/app/shell/bottom-panel";
import { useHints, useWorkbenchKeys } from "@/app/shell/keys";
import { ServerSwitcher } from "@/app/shell/server-switcher";
import { PhoneSessionHeader } from "@/app/shell/session-header";
import {
	ActivityBar,
	SidePanel,
	useSidebars,
	VIEWS,
	type ViewId,
	viewLabel,
} from "@/app/shell/sidebars";
import { PhoneStatusButton, PhoneStatusSheet, StatusBar } from "@/app/shell/status-bar";
import { useIsPhone } from "@/app/shell/use-media";
import { workbenchCommands } from "@/app/shell/workbench-commands";
import { newSessionTab, sessionTab, Workbench, ProjectTiles } from "@/app/shell/workbench-tiles";
import { WaitingCount } from "@/modules/machines";
import { NotificationBell } from "@/modules/notifications";
import { useFeatureFlags } from "@/modules/platform";
import {
	openWindow,
	type PluginViewItem,
	PluginView,
	setHost,
	useContributions,
} from "@/modules/plugins";
import {
	clientClass,
	CommandPalette,
	BranchPicker,
	fileStatusKey,
	fileStatusOf,
	FileTree,
	GitView,
	QuickOpen,
	SearchView,
	projectFileDiff,
	type GitOpeners,
	type OpenTab,
	type Tab,
	inListOrder,
	OpenFolderDialog,
	ProjectSwitcher,
	type TilesHandle,
	useOpenFolder,
	useProject,
	useProjects,
	useProjectRepository,
} from "@/modules/projects";
import {
	ChatContextProvider,
	SessionList,
	SessionsAddMenu,
	useEditedFiles,
	useSession,
	useStartTerminal,
	useWaitingSessions,
} from "@/modules/sessions";
import { RepositoryPin } from "@/modules/tools";
import { problemCode } from "@/shared/api/fetch";
import type { Session, SessionFolder } from "@/shared/api/generated/ahab.schemas";
import { app } from "@/shared/desktop/bridge";
import { useDesktopApp } from "@/shared/desktop/hooks";
import { i18n, useTranslation } from "@/shared/i18n";
import type { ActionId } from "@/shared/keymap/model";
import { rememberFileOpened, rememberProject } from "@/shared/preferences/store";
import { Button } from "@/shared/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";
import { SidebarHeading as Heading } from "@/shared/ui/sidebar-heading";
import { toastError } from "@/shared/ui/toast";

/** What a phone's list shows. */
/** A phone's list: one of Ahab's views, or a plugin's (`plugin:<id>:<view>`). */
type PhoneView = ViewId;

/**
 * A project, laid out like VS Code: the project switcher and search bar on
 * top; the activity bar and sidebar on the left; tiles holding the
 * project's windows (sessions, files, diffs). On a phone, one screen at a time, like a mail app: a list (sessions,
 * files, Git or search) first, then the work full-screen, with a way back.
 */
export function WorkbenchPage({
	project: reference,
	session: sessionId,
}: {
	project: string;
	session?: string;
}) {
	const { t } = useTranslation("shell");
	const navigate = useNavigate();
	const phone = useIsPhone();
	const projects = useProjects();
	const projectQuery = useProject(reference);
	const waiting = useWaitingSessions();
	// In the desktop app, the tray's dot and the window's badge.
	const desktop = useDesktopApp();
	const pathname = useRouterState({ select: (state) => state.location.pathname });
	const needYou = Object.values(waiting.data?.projects ?? {}).reduce(
		(sum, count) => sum + count,
		0,
	);
	useEffect(() => {
		if (desktop) void app.badge(needYou);
	}, [desktop, needYou]);
	const openSession = useSession(sessionId ?? "");
	// What the open session's agents edited, marked in the Git view.
	const edited = useEditedFiles(sessionId);
	const [kind] = useState(clientClass);
	// The tiles' handle, once they are ready (a callback ref, kept in state).
	const [tiles, setTiles] = useState<TilesHandle | null>(null);
	const extras = useContributions();
	// Core features switched off: their views, commands and keys go.
	const flag = useFeatureFlags();
	const terminalOn = flag("terminal");
	const filesOn = flag("files");
	const gitOn = flag("git");
	const searchOn = flag("search");
	const on = useMemo(
		() => ({ terminal: terminalOn, files: filesOn, git: gitOn, search: searchOn }),
		[terminalOn, filesOn, gitOn, searchOn],
	);
	const hint = useHints();
	const bars = useSidebars(
		extras.views.map((view) => view.key),
		(["files", "git", "search"] as const).filter((view) => !on[view]),
	);
	// Phones show one list at a time: sessions, files, Git or search.
	const [phoneView, setPhoneView] = useState<PhoneView>("sessions");
	// Phones: the status bar's items, in a sheet from the top bar.
	const [statusSheet, setStatusSheet] = useState(false);
	const [quickOpen, setQuickOpen] = useState(false);
	const [branches, setBranches] = useState(false);
	// "Run a command": the command asked for, in a terminal of its own.
	const [running, setRunning] = useState(false);
	// Bumped by ⌘⇧F, so the Search view takes the focus again.
	const [searchFocus, setSearchFocus] = useState(0);
	// "Find in folder" from the file tree: the folder the Search view starts in.
	const [searchFolder, setSearchFolder] = useState<string>();
	// Phones: the list, or the work (a window: a session, a file, a diff).
	const [screen, setScreen] = useState<"list" | "work">(() => (sessionId ? "work" : "list"));
	const [palette, setPalette] = useState(false);
	const [openingFolder, setOpeningFolder] = useState(false);
	const [active, setActive] = useState<Record<string, unknown> | undefined>();
	const [tabs, setTabs] = useState<OpenTab[]>([]);
	const openFolder = useOpenFolder();

	const data = projectQuery.data;
	const session = sessionId ? openSession.data : undefined;
	const project = data;
	// What a phone shows: an open session is always the work.
	const phoneScreen = sessionId ? "work" : screen;
	const {
		repository,
		repositories: found,
		ready: gitReady,
		choose: chooseRepository,
		status: git,
	} = useProjectRepository(on.git ? project : undefined);

	useEffect(() => {
		if (data) rememberProject(data.id);
	}, [data]);

	// Showing a view opens its sidebar (or, on a phone, the list).
	const { show: showView, toggle: toggleSidebar } = bars;
	const show = useCallback(
		(next: ViewId) => {
			if (phone) {
				setPhoneView(next);
				setScreen("list");
			} else showView(next);
		},
		[phone, showView],
	);

	const goTo = useCallback(
		(next: { session?: string }) => {
			if (!data) return;
			setScreen(next.session ? "work" : "list");
			if (next.session)
				void navigate({
					to: "/p/$project/s/$session",
					params: { project: data.id, session: next.session },
				});
			else void navigate({ to: "/p/$project", params: { project: data.id } });
		},
		[data, navigate],
	);

	const showSession = useCallback(
		(next: Session, keep?: boolean) => {
			if (next.projectId === project?.id) tiles?.open({ ...sessionTab(next), keep });
			goTo({ session: next.id });
		},
		[goTo, project?.id, tiles],
	);
	// The open session was archived or deleted: its window closes.
	const leave = useCallback(() => {
		if (sessionId) tiles?.close(`session:${sessionId}`);
		goTo({});
	}, [goTo, sessionId, tiles]);
	// A new session is one more window, perhaps for a folder.
	const newSessionIn = useCallback(
		(folder?: SessionFolder) => {
			tiles?.open(newSessionTab(folder));
			if (phone) setScreen("work");
		},
		[tiles, phone],
	);
	const newSession = useCallback(() => newSessionIn(), [newSessionIn]);
	// The bottom panel (Problems, Output): open or closed per project.
	const panel = usePanel(project?.id);
	const { request: panelRequest } = panel;
	const askPanel = useCallback(
		(request: Parameters<typeof panelRequest>[0]) => {
			panelRequest(request);
			// A phone shows the panel as the work.
			if (phone && request.kind !== "close" && request.kind !== "toggle") setScreen("work");
		},
		[panelRequest, phone],
	);
	const openProblems = useCallback(() => askPanel({ kind: "show", tab: "problems" }), [askPanel]);
	const togglePanel = useCallback(() => askPanel({ kind: "toggle" }), [askPanel]);
	const showOutput = useCallback(() => askPanel({ kind: "show", tab: "output" }), [askPanel]);
	const toggleProblems = useCallback(
		() => askPanel({ kind: "toggleTab", tab: "problems" }),
		[askPanel],
	);
	// A session started in a New session window takes that window's place.
	const sessionStarted = useCallback(
		(started: Session, from?: string) => {
			tiles?.open({ ...sessionTab(started), keep: true }, from);
			showSession(started);
		},
		[tiles, showSession],
	);

	// The route's session is in front: its window opens, or comes forward.
	const routeSession = session?.id;
	const routeTitle = session?.title;
	useEffect(() => {
		if (routeSession && routeTitle && tiles)
			tiles.open(sessionTab({ id: routeSession, title: routeTitle }));
	}, [routeSession, routeTitle, tiles]);

	// The window in front decides the route: a session's, or the project.
	const windowActive = useCallback(
		(_id: string | undefined, params: Record<string, unknown> | undefined) => {
			setActive(params);
			const target = typeof params?.sessionId === "string" ? params.sessionId : undefined;
			if (!data || target === sessionId) return;
			if (target)
				void navigate({
					to: "/p/$project/s/$session",
					params: { project: data.id, session: target },
				});
			else void navigate({ to: "/p/$project", params: { project: data.id } });
		},
		[data, sessionId, navigate],
	);

	// A terminal is a session too: started here, shown in its own window.
	const startTerminal = useStartTerminal();
	const startTerminalNow = startTerminal.mutate;
	const newTerminalIn = useCallback(
		(folder?: SessionFolder) => {
			if (!project) return;
			startTerminalNow(
				{ id: project.id, data: { cols: 100, rows: 30, folderId: folder?.id ?? null } },
				{ onSuccess: (started) => sessionStarted(started), onError: toastError },
			);
		},
		[project, startTerminalNow, sessionStarted],
	);
	const newTerminal = useCallback(() => newTerminalIn(), [newTerminalIn]);

	const openFile = (path: string, line?: number, beside = false, keep = false) => {
		if (!project) return;
		tiles?.openFile(project.id, project.name, path, line, beside, keep);
		rememberFileOpened(project.id, path);
		// On a phone the tiles stay mounted under the list.
		if (phone) setScreen("work");
	};
	// Windows open files through the newest `openFile`.
	const opening = useRef(openFile);
	useEffect(() => {
		opening.current = openFile;
	});
	const openFromWindow = useCallback(
		(path: string, line?: number) => opening.current(path, line, false, true),
		[],
	);

	const openFileDiff = useCallback(
		async (file: string) => {
			if (!project) return;
			const diff = await projectFileDiff(project.id, file);
			if (!diff) throw new Error(t("tools.noDiff", { ns: "sessions" }));
			tiles?.openDiff(diff);
			if (phone) setScreen("work");
		},
		[phone, project, tiles, t],
	);
	const context = useMemo(
		() => ({
			project,
			onOpenFile: openFromWindow,
			onOpenFileDiff: on.git ? openFileDiff : undefined,
			onBrowse: () => show("files"),
			onCommand: () => setPalette(true),
			onSessionStarted: sessionStarted,
			onNewSession: newSession,
			onNewTerminal: on.terminal ? newTerminal : undefined,
			phone,
			terminals: on.terminal,
		}),
		[
			on.git,
			openFileDiff,
			phone,
			project,
			show,
			sessionStarted,
			newSession,
			newTerminal,
			openFromWindow,
			on.terminal,
		],
	);

	// Context for chats: the sessions open here, and where it goes.
	const openSessions = useMemo(
		() =>
			tabs.map((tab) => tab.params.sessionId).filter((id): id is string => typeof id === "string"),
		[tabs],
	);
	const showSessionWindow = useCallback(
		(shown: { id: string; title: string }) => {
			tiles?.open({ ...sessionTab(shown), keep: true });
			if (phone) setScreen("work");
		},
		[tiles, phone],
	);
	const newSessionWith = useCallback(
		(draftKey: string) => {
			tiles?.open(newSessionTab(undefined, draftKey));
			if (phone) setScreen("work");
		},
		[tiles, phone],
	);

	const findInFiles = useCallback(() => {
		show("search");
		setSearchFolder(undefined);
		setSearchFocus((n) => n + 1);
	}, [show]);
	const togglePalette = useCallback(() => setPalette((open) => !open), []);
	const openQuick = useCallback(() => setQuickOpen(true), []);
	useWorkbenchKeys({
		palette: togglePalette,
		"sidebar.left": () => toggleSidebar("left"),
		"sidebar.right": () => toggleSidebar("right"),
		"session.new": newSession,
		"terminal.new": on.terminal ? newTerminal : undefined,
		"window.close": () => tiles?.closeActive(),
		"window.reopen": () => tiles?.reopenClosed(),
		"window.next": () => tiles?.cycle(1),
		"window.previous": () => tiles?.cycle(-1),
		"window.split": () => tiles?.splitActive("right"),
		"files.quickOpen": on.files ? openQuick : undefined,
		"search.find": on.search ? findInFiles : undefined,
		"panel.toggle": project ? togglePanel : undefined,
		"panel.problems": project ? openProblems : undefined,
		"panel.output": project ? showOutput : undefined,
		// Alt+1…9: the projects in the order the switcher lists them.
		...Object.fromEntries(
			inListOrder(projects.data?.items ?? [])
				.slice(0, 9)
				.map((listed, index) => [
					`project.${index + 1}`,
					() => {
						if (listed.id !== data?.id)
							void navigate({ to: "/p/$project", params: { project: listed.id } });
					},
				]),
		),
	});

	// What plugins act on: the project, and how to open windows here.
	const openPluginWindow = useCallback(
		(item: PluginViewItem, keep = false) => {
			if (!item.opens) return;
			if (keep) tiles?.keepActive();
			else openWindow(item.plugin, item.opens.window, item.opens.params);
		},
		[tiles],
	);
	useEffect(() => {
		setHost({
			project: project
				? {
						id: project.id,
						machineId: project.machineId,
						name: project.name,
						path: project.path,
					}
				: undefined,
			openWindow: (tab) => {
				tiles?.open(tab as Tab);
				// A phone shows the list or the work: a window opened is the work.
				if (phone) setScreen("work");
			},
			openSession: (started) => tiles?.open({ ...sessionTab(started), keep: true }),
			showView: (key) => show(key),
		});
	}, [project, tiles, show, phone]);

	const projectList = projects.data?.items;
	const commands = useMemo(
		() =>
			workbenchCommands({
				project: data,
				projects: projectList ?? [],
				tiles,
				openProject: (id) => void navigate({ to: "/p/$project", params: { project: id } }),
				show,
				newSession,
				newTerminal,
				quickOpen: openQuick,
				findInFiles,
				problems: openProblems,
				togglePanel,
				output: showOutput,
				goToProblem:
					active?.kind === "file" && typeof active.projectId === "string"
						? (direction) =>
								fileStatusOf(
									fileStatusKey(String(active.projectId), String(active.path)),
								)?.goToProblem?.(direction)
						: undefined,
				switchBranch: () => setBranches(true),
				runCommand: () => setRunning(true),
				toggleSidebar,
				openFolder: () => setOpeningFolder(true),
				settings: (page) => void navigate({ to: `/settings/${page}` }),
				on: (feature) => on[feature],
				hint,
			}).concat(
				extras.palette.map((item) => ({
					id: item.key,
					group: t("commands.groups.plugins"),
					label: item.title,
					hint: item.keywords?.join(" "),
					run: item.run,
				})),
			),
		[
			extras.palette,
			data,
			projectList,
			tiles,
			navigate,
			show,
			newSession,
			newTerminal,
			openQuick,
			findInFiles,
			openProblems,
			togglePanel,
			showOutput,
			active,
			toggleSidebar,
			on,
			hint,
			t,
		],
	);

	if (projectQuery.isError) {
		return (
			<section className="mx-auto max-w-lg px-6 pt-24">
				<h1 className="font-heading text-heading text-2xl font-semibold">
					{problemCode(projectQuery.error) === "project_not_found"
						? t("workbench.projectNotFound")
						: t("workbench.projectUnloadable")}
				</h1>
				<Button asChild variant="outline" className="mt-4">
					<Link to="/">{t("backToProjects")}</Link>
				</Button>
			</section>
		);
	}

	const activeFile =
		active?.kind === "file" && active.projectId === project?.id ? String(active.path) : undefined;
	const waitingHere = data ? waiting.data?.projects[data.id] : undefined;
	// The focused file window, whose items the status bar shows.
	const activeFileKey =
		active?.kind === "file" && typeof active.projectId === "string"
			? fileStatusKey(active.projectId, String(active.path))
			: undefined;
	const panelShown = Boolean(project) && panel.state.open;
	const panelFull = phone || panel.state.maximised;
	// A problem opens its file there.
	const openProblemAt = (projectId: string, path: string, line?: number) => {
		if (projectId === project?.id) openFile(path, line);
		if (phone) askPanel({ kind: "close" });
	};

	const openDiff: GitOpeners["onOpenDiff"] = (change, group, commit) => {
		if (!project) return;
		tiles?.openDiff({
			projectId: project.id,
			path: change.path,
			oldPath: change.oldPath ?? undefined,
			group,
			commit,
			prefix: git.data?.prefix ?? "",
			repository,
		});
		if (phone) setScreen("work");
	};

	// The open file windows of this project at `path` or inside it.
	const filesUnder = (path: string) =>
		tabs.filter((tab) => {
			const open = String(tab.params.path ?? "");
			return (
				tab.params.kind === "file" &&
				tab.params.projectId === project?.id &&
				(open === path || open.startsWith(`${path}/`))
			);
		});

	const fileTree = project ? (
		project.availability === "available" ? (
			<FileTree
				key={project.id}
				projectId={project.id}
				selected={activeFile}
				onOpen={(path, keep) => openFile(path, undefined, false, keep)}
				onOpenBeside={phone ? undefined : (path) => openFile(path, undefined, true, true)}
				root={project.path}
				readOnly={project.mode === "readOnly"}
				onRenamed={(from, to) => {
					for (const tab of filesUnder(from)) {
						tiles?.close(tab.id);
						openFile(to + String(tab.params.path).slice(from.length), undefined, false, true);
					}
				}}
				onDeleted={(path) => {
					for (const tab of filesUnder(path)) tiles?.close(tab.id);
				}}
				onFindInFolder={(path) => {
					setSearchFolder(path);
					setSearchFocus((n) => n + 1);
					show("search");
				}}
			/>
		) : (
			<p className="text-muted-foreground px-3 text-sm">
				{project.availability === "missing"
					? t("workbench.folderGone")
					: t("workbench.machineOffline", { machine: project.machineName })}
			</p>
		)
	) : null;

	// Agents and terminals in one list; the + starts either, or a folder.
	// The list fills what the heading leaves, so all of it takes drops,
	// without growing taller than the panel.
	const sessionList = (
		<div className="flex min-h-full flex-col">
			<Heading
				action={
					project && data ? (
						<SessionsAddMenu
							project={data.id}
							onNewAgent={newSession}
							onNewTerminal={on.terminal ? newTerminal : undefined}
						/>
					) : null
				}
			>
				{t("views.sessions")}
			</Heading>
			{data ? (
				<SessionList
					project={data.id}
					selected={sessionId}
					onOpen={showSession}
					onGone={(gone) => tiles?.close(`session:${gone.id}`)}
					terminals={on.terminal}
					onNewIn={(what, folder) =>
						what === "terminal" ? newTerminalIn(folder) : newSessionIn(folder)
					}
				/>
			) : null}
		</div>
	);

	/** What a view shows, in whichever sidebar holds it. */
	const viewContent = (view: string) => (
		<div
			className={cn(
				"min-h-0 flex-1",
				// Git lays out its own panes (the graph scrolls on its own).
				view === "git" ? "flex flex-col overflow-hidden" : "overflow-auto pb-4",
			)}
		>
			{view.startsWith("plugin:") ? (
				<>
					<Heading>{viewLabel(view, extras.views)}</Heading>
					<PluginView viewKey={view} />
				</>
			) : view === "sessions" ? (
				sessionList
			) : view === "files" ? (
				<>
					{project?.availability !== "available" && <Heading>{t("views.files")}</Heading>}
					{fileTree}
				</>
			) : !project ? null : project.availability !== "available" ? (
				<>
					<Heading>{viewLabel(view, extras.views)}</Heading>
					<p className="text-muted-foreground px-3 text-sm">
						{project.availability === "missing"
							? t("workbench.folderGone")
							: t("workbench.machineOffline", { machine: project.machineName })}
					</p>
				</>
			) : view === "git" ? (
				<>
					<Heading>{t("views.git")}</Heading>
					{gitReady ? (
						<GitView
							key={project.id}
							project={project}
							repository={repository}
							repositories={found}
							onChooseRepository={chooseRepository}
							edited={edited.data?.paths}
							onOpenDiff={openDiff}
							onOpenFile={(path) => openFile(path)}
							checkoutActions={RepositoryPin}
						/>
					) : (
						<p className="text-muted-foreground px-3 text-sm">{t("workbench.findingRepository")}</p>
					)}
				</>
			) : (
				<>
					<Heading>{t("views.search")}</Heading>
					<SearchView
						key={`${project.id}:${searchFocus}`}
						project={project}
						folder={searchFolder}
						autoFocus={searchFocus > 0}
						onOpen={(path, line, beside) => openFile(path, line, beside)}
					/>
				</>
			)}
		</div>
	);

	// A phone's list: sessions or files.
	const phoneViews = (["sessions", "files", "git", "search"] as const).filter(
		(item) => item === "sessions" || on[item],
	);
	// Plugins' views and windows, behind a More tab.
	const phonePlugins = extras.views;
	const phonePlugin = phonePlugins.find((item) => item.key === phoneView);
	const phoneList = (
		<>
			<div
				role="tablist"
				aria-label={t("workbench.views")}
				className="bg-muted mx-2 mb-1 grid auto-cols-fr grid-flow-col gap-1 rounded-lg p-1"
			>
				{phoneViews.map((item) => (
					<button
						key={item}
						type="button"
						role="tab"
						aria-selected={phoneView === item}
						onClick={() => setPhoneView(item)}
						className={cn(
							"h-9 rounded-md text-sm font-medium",
							phoneView === item ? "bg-background text-heading shadow-sm" : "text-muted-foreground",
						)}
					>
						{t(VIEWS[item].label)}
					</button>
				))}
				{phonePlugins.length ? (
					<DropdownMenu>
						<DropdownMenuTrigger
							role="tab"
							aria-selected={Boolean(phonePlugin)}
							className={cn(
								"flex h-9 min-w-0 items-center justify-center gap-1 rounded-md px-2 text-sm font-medium",
								phonePlugin ? "bg-background text-heading shadow-sm" : "text-muted-foreground",
							)}
						>
							<span className="truncate">{phonePlugin?.title ?? t("workbench.more")}</span>
							<ChevronDown className="size-3.5 shrink-0" />
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end">
							{phonePlugins.map((item) => (
								<DropdownMenuItem
									key={item.key}
									onSelect={() =>
										item.Component ? setPhoneView(item.key) : openPluginWindow(item)
									}
								>
									{item.icon}
									{item.title}
								</DropdownMenuItem>
							))}
						</DropdownMenuContent>
					</DropdownMenu>
				) : null}
			</div>
			{viewContent(
				(phoneViews as readonly string[]).includes(phoneView) ||
					phonePlugins.some((item) => item.key === phoneView && item.Component)
					? phoneView
					: "sessions",
			)}
		</>
	);

	const side = (which: "left" | "right") => {
		const bar = bars.layout[which];
		if (!bar.open || !bar.active) return null;
		return (
			<SidePanel
				side={which}
				width={bar.width}
				label={viewLabel(bar.active, extras.views)}
				onResize={(width) => bars.resize(which, width)}
			>
				{viewContent(bar.active)}
			</SidePanel>
		);
	};
	const badges = { sessions: waitingHere };
	const changed = git.data?.repository
		? git.data.staged.length +
			git.data.changes.length +
			git.data.untracked.length +
			git.data.conflicts.length
		: 0;
	const counts = { git: changed };

	return (
		<Workbench.Provider value={context}>
			<ChatScope
				projectId={data?.id}
				openSessions={openSessions}
				onShowSession={showSessionWindow}
				onNewSessionWith={newSessionWith}
			>
				<div className="bg-background flex h-svh flex-col overflow-hidden">
					{phone && session ? (
						<PhoneSessionHeader
							terminals={on.terminal}
							session={session}
							project={project}
							tabs={tabs}
							onBack={leave}
							onLeave={leave}
							onFocusTab={(id) => tiles?.focus(id)}
							onCloseTab={(id) => tiles?.close(id)}
							onNewSession={newSession}
							onStatus={project ? () => setStatusSheet(true) : undefined}
						/>
					) : (
						<header className="bg-muted/40 border-border grid h-12 shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-1 border-b px-1.5 max-md:box-content max-md:pt-[env(safe-area-inset-top)] md:h-10 md:grid-cols-[1fr_minmax(12rem,34rem)_1fr] md:gap-3 md:px-2">
							<div className="flex min-w-0 items-center gap-1">
								{phone && phoneScreen !== "list" ? (
									<button
										type="button"
										onClick={() => (session ? goTo({}) : setScreen("list"))}
										aria-label={
											phoneView === "files" && !session
												? t("workbench.backToFiles")
												: t("backToSessions")
										}
										className="text-foreground hover:bg-accent focus-visible:ring-ring relative grid size-11 place-items-center rounded-md outline-none focus-visible:ring-2"
									>
										<ArrowLeft className="size-5" />
										{waitingHere ? (
											<span className="bg-warning absolute top-2 right-2 size-2 rounded-full" />
										) : null}
									</button>
								) : (
									<Link to="/" aria-label="Ahab" className="px-1.5">
										<img src="/ahab-mark.svg" alt="" className="h-4 w-auto dark:hidden" />
										<img
											src="/ahab-mark-white.svg"
											alt=""
											className="hidden h-4 w-auto dark:block"
										/>
									</Link>
								)}
								{phone ? null : <ServerSwitcher path={pathname} />}
								<ProjectSwitcher
									projects={projects.data?.items ?? []}
									current={data}
									waiting={waiting.data?.projects}
									onSwitch={(next) =>
										void navigate({ to: "/p/$project", params: { project: next.id } })
									}
									onOpenFolder={() => setOpeningFolder(true)}
									onManage={() => void navigate({ to: "/settings/projects" })}
									onOpenInNewWindow={
										desktop && data ? () => void app.newWindow(`/p/${data.id}`) : undefined
									}
									shortcut={(index) =>
										index >= 0 && index < 9 ? hint(`project.${index + 1}` as ActionId) : undefined
									}
								/>
							</div>
							<button
								type="button"
								onClick={() => setPalette(true)}
								aria-label={t("workbench.search")}
								className="bg-background border-border text-muted-foreground hover:text-foreground focus-visible:ring-ring hover:max-md:bg-accent flex h-11 items-center gap-2 justify-self-end rounded-md px-2.5 text-[0.8125rem] outline-none focus-visible:ring-2 max-md:w-11 max-md:justify-center max-md:bg-transparent max-md:px-0 md:h-7 md:justify-self-stretch md:border"
							>
								<Search className="size-4 md:size-3.5" />
								<span className="truncate max-md:hidden">{t("workbench.search")}</span>
								{hint("palette") ? (
									<kbd className="ml-auto font-mono text-[0.6875rem] max-md:hidden">
										{hint("palette")}
									</kbd>
								) : null}
							</button>
							<div className="flex items-center justify-end gap-0.5 md:gap-1">
								{phone && project ? (
									<PhoneStatusButton project={project} onOpen={() => setStatusSheet(true)} />
								) : null}
								{phone ? null : (
									<>
										<SidebarToggle
											label={withKey(t("workbench.toggleLeft"), hint("sidebar.left"))}
											open={bars.layout.left.open}
											onClick={() => toggleSidebar("left")}
										>
											<PanelLeft />
										</SidebarToggle>
										<SidebarToggle
											label={withKey(t("workbench.toggleRight"), hint("sidebar.right"))}
											open={bars.layout.right.open}
											onClick={() => toggleSidebar("right")}
										>
											<PanelRight />
										</SidebarToggle>
									</>
								)}
								<NotificationBell phone={phone} />
								<Link
									to="/settings"
									className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring flex h-11 min-w-11 items-center justify-center gap-1.5 rounded-md px-2 text-sm outline-none focus-visible:ring-2 md:h-7 md:min-w-7 md:px-1.5"
									aria-label={t("settings.title")}
								>
									<Settings className="size-4" />
									<WaitingCount />
								</Link>
							</div>
						</header>
					)}
					<div className="relative flex min-h-0 flex-1">
						{phone ? (
							phoneScreen === "list" ? (
								<aside
									aria-label={t("workbench.phoneList")}
									className="bg-background flex min-w-0 flex-1 flex-col"
								>
									{phoneList}
								</aside>
							) : null
						) : (
							<>
								<ActivityBar
									side="left"
									bar={bars.layout.left}
									badges={badges}
									counts={counts}
									onShow={show}
									onHide={() => toggleSidebar("left")}
									onMove={bars.move}
									plugins={extras.views}
									onOpenWindow={openPluginWindow}
								/>
								{side("left")}
							</>
						)}
						<main
							className={cn(
								"flex min-w-0 flex-1 flex-col",
								phone && phoneScreen !== "work" && "hidden",
							)}
						>
							<div className={cn("min-h-0 flex-1", panelShown && panelFull && "hidden")}>
								{project ? (
									<ProjectTiles
										key={project.id}
										ref={setTiles}
										project={project}
										kind={kind}
										onActive={windowActive}
										onTabs={setTabs}
										hideTabs={phone}
										onProblemsRetired={openProblems}
									/>
								) : null}
							</div>
							{panelShown && project ? (
								<BottomPanel
									key={project.id}
									state={panel.state}
									onRequest={askPanel}
									project={project}
									output={panel.output}
									onOutput={panel.setOutput}
									onOpenFile={openProblemAt}
									phone={phone}
									hints={{
										toggle: hint("panel.toggle"),
										problems: hint("panel.problems"),
										output: hint("panel.output"),
									}}
								/>
							) : null}
						</main>
						{phone ? null : (
							<>
								{side("right")}
								<ActivityBar
									side="right"
									bar={bars.layout.right}
									badges={badges}
									counts={counts}
									onShow={show}
									onHide={() => toggleSidebar("right")}
									onMove={bars.move}
									plugins={extras.views}
									onOpenWindow={openPluginWindow}
								/>
							</>
						)}
					</div>
					{project ? (
						phone ? (
							<PhoneStatusSheet
								open={statusSheet}
								onOpenChange={setStatusSheet}
								project={project}
								repository={repository}
								git={on.git}
								onBranch={() => setBranches(true)}
								onProblems={toggleProblems}
								fileKey={activeFileKey}
							/>
						) : (
							<StatusBar
								project={project}
								repository={repository}
								git={on.git}
								onBranch={() => setBranches(true)}
								onProblems={toggleProblems}
								fileKey={activeFileKey}
							/>
						)
					) : null}
					<CommandPalette open={palette} onOpenChange={setPalette} commands={commands} />
					{project ? (
						<>
							<QuickOpen
								projectId={project.id}
								open={on.files && quickOpen}
								onOpenChange={setQuickOpen}
								onOpen={(path, beside) => openFile(path, undefined, beside)}
							/>
							<NameDialog
								open={running}
								onOpenChange={setRunning}
								title={t("workbench.runCommand.title")}
								label={t("workbench.runCommand.label")}
								action={t("workbench.runCommand.action")}
								pending={startTerminal.isPending}
								error={startTerminal.error?.message}
								onSubmit={(command) =>
									startTerminal.mutate(
										{ id: project.id, data: { command, cols: 100, rows: 30 } },
										{
											onSuccess: (started) => {
												setRunning(false);
												sessionStarted(started);
											},
										},
									)
								}
							/>
							<BranchPicker
								project={project}
								repository={repository}
								open={on.git && branches}
								onOpenChange={setBranches}
							/>
						</>
					) : null}
					<OpenFolderDialog
						open={openingFolder}
						onOpenChange={(open) => {
							setOpeningFolder(open);
							openFolder.reset();
						}}
						title={t("workbench.openFolder.title")}
						description={t("workbench.openFolder.description")}
						action={t("workbench.openFolder.action")}
						pending={openFolder.isPending}
						error={openFolder.error?.message}
						onPick={(machine, path) =>
							openFolder.mutate(
								{ data: { machine, path } },
								{
									onSuccess: (opened) => {
										setOpeningFolder(false);
										void navigate({ to: "/p/$project", params: { project: opened.project.id } });
									},
								},
							)
						}
					/>
				</div>
			</ChatScope>
		</Workbench.Provider>
	);
}

/** Adding context to chats, once there is a project to add it in. */
function ChatScope({
	projectId,
	children,
	...rest
}: {
	projectId?: string;
	openSessions: string[];
	onShowSession: (session: { id: string; title: string }) => void;
	onNewSessionWith: (draftKey: string) => void;
	children: React.ReactNode;
}) {
	if (!projectId) return children;
	return (
		<ChatContextProvider projectId={projectId} {...rest}>
			{children}
		</ChatContextProvider>
	);
}

/** A label with its key, if it has one: "Toggle the left sidebar (⌘B)". */
function withKey(label: string, key: string | undefined) {
	return key ? i18n.t("withKey", { label, key }) : label;
}

/** A title-bar button that opens or closes a sidebar. */
function SidebarToggle({
	label,
	open,
	onClick,
	children,
}: {
	label: string;
	open: boolean;
	onClick: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			aria-label={label}
			aria-pressed={open}
			title={label}
			className={cn(
				"hover:bg-accent focus-visible:ring-ring grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2 [&_svg]:size-4",
				open ? "text-foreground" : "text-muted-foreground",
			)}
		>
			{children}
		</button>
	);
}
