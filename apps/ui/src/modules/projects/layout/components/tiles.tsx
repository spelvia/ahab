import "dockview-react/dist/styles/dockview.css";
import {
	type DockviewApi,
	DockviewReact,
	type IDockviewHeaderActionsProps,
	type IDockviewPanelHeaderProps,
	type IDockviewPanelProps,
	type IWatermarkPanelProps,
	themeDark,
} from "dockview-react";
import {
	createContext,
	forwardRef,
	useContext,
	useEffect,
	useImperativeHandle,
	useMemo,
	useRef,
} from "react";

import { FileView } from "@/modules/projects/files";
import { DiffView } from "@/modules/projects/git";
import type { LayoutStore } from "@/modules/projects/layout/api";
import {
	baseName,
	type DiffTab,
	diffTabId,
	fileTabId,
	isPreview,
	keepOpen,
	openTab,
	type TabParams,
	withoutRetired,
} from "@/modules/projects/layout/model";
import { problemCode } from "@/shared/api/fetch";
import { externalName } from "@/shared/editor/registry";
import { t as translate, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** A tab to open: its stable ID, which panel shows it, and its title. */
export interface Tab {
	id: string;
	component: string;
	title: string;
	params: Record<string, unknown>;
	/** Keep this window instead of opening a replaceable preview. */
	keep?: boolean;
}

/** An open tab, as the tiles report it. */
export interface OpenTab {
	id: string;
	component: string;
	title: string;
	params: Record<string, unknown>;
	active: boolean;
}

/** What the workbench asks of the tiles. */
export interface TilesHandle {
	/** Opens a file (at `line`), or brings it forward. */
	/** `beside`: in a new tile to the right of the current one. */
	openFile: (
		projectId: string,
		projectName: string,
		path: string,
		line?: number,
		beside?: boolean,
		keep?: boolean,
	) => void;
	/** Opens a changed file's diff, or brings it forward. */
	openDiff: (tab: Omit<DiffTab, "kind">) => void;
	/** Opens `tab`, or focuses it when open; in place of `replace` if given. */
	open: (tab: Tab, replace?: string) => void;
	/** Keeps the active preview open. */
	keepActive: () => void;
	/** Closes a tab. */
	close: (id: string) => void;
	/** Shows a tab. */
	focus: (id: string) => void;
	/** Closes the tab in front. */
	closeActive: () => void;
	/** Opens the tab closed last again. */
	reopenClosed: () => void;
	/** Shows the next (1) or previous (-1) tab of the group in front. */
	cycle: (step: 1 | -1) => void;
	/** Moves the tab in front into a new tile beside it. */
	splitActive: (side: "right" | "bottom") => void;
}

/** Closed tabs kept to reopen, most recent last. */
const REOPEN_LIMIT = 20;

/** Panels other modules add, by name (sessions, later terminals). */
export type Panels = Record<string, React.FunctionComponent<IDockviewPanelProps>>;

const theme = { ...themeDark, name: "ahab", className: "dockview-theme-dark dockview-theme-ahab" };

/** What file windows need to know of the projects around them. */
interface TilesInfo {
	/** Each project's mode: read-only ones offer no Edit switch. */
	readOnly: (projectId: string) => boolean;
}

const TilesContext = createContext<TilesInfo>({ readOnly: () => false });

/**
 * A file's window; links in it, and go to definition, open the files they
 * lead to as windows. (Typing in a preview keeps it open, as in any window.)
 */
function FilePanel({ params, containerApi }: IDockviewPanelProps<TabParams>) {
	const info = useContext(TilesContext);
	const open = (path: string, extra: Partial<TabParams>) => {
		const linked: TabParams = {
			kind: "file",
			projectId: params.projectId,
			projectName: params.projectName,
			path,
			jump: Date.now(),
			...extra,
		};
		openTab(containerApi, {
			id: fileTabId(params.projectId, path),
			component: "file",
			title: externalName(path) ?? baseName(path),
			params: linked,
		});
	};
	return (
		<FileView
			projectId={params.projectId}
			projectName={params.projectName}
			path={params.path}
			anchor={params.anchor}
			line={params.line}
			jump={params.jump}
			readOnlyProject={info.readOnly(params.projectId)}
			onOpenFile={(path, anchor) => open(path, { anchor })}
			onOpenLocation={(path, line) => open(path, { line })}
		/>
	);
}

/** A diff window; its file opens beside it. */
function DiffPanel({ params, api, containerApi }: IDockviewPanelProps<DiffTab>) {
	// The file, as a path of the project (when it is in its folder).
	const relative = params.repository
		? `${params.repository}/${params.path}`
		: params.path.startsWith(params.prefix)
			? params.path.slice(params.prefix.length)
			: undefined;
	return (
		<DiffView
			projectId={params.projectId}
			repository={params.repository}
			path={params.path}
			oldPath={params.oldPath}
			group={params.group}
			commit={params.commit}
			filePath={relative}
			onOpenFile={
				relative === undefined || params.group === "commit"
					? undefined
					: () => {
							const id = fileTabId(params.projectId, relative);
							const source = containerApi.getPanel(api.id);
							if (source) keepOpen(source);
							openTab(containerApi, {
								id,
								component: "file",
								title: baseName(relative),
								params: {
									kind: "file",
									projectId: params.projectId,
									projectName: "",
									path: relative,
								} satisfies TabParams,
								position: { referencePanel: api.id, direction: "within" },
							});
						}
			}
		/>
	);
}

const builtIn = { file: FilePanel, diff: DiffPanel };

/** A diff's tab title: the file and which sides. */
function diffTitle(tab: Omit<DiffTab, "kind">): string {
	const name = baseName(tab.path);
	switch (tab.group) {
		case "staged":
			return translate("tiles.diffTitle.staged", { ns: "projects", name });
		case "commit":
			return translate("tiles.diffTitle.commit", {
				ns: "projects",
				name,
				commit: (tab.commit ?? "").slice(0, 7),
			});
		case "conflicts":
			return translate("tiles.diffTitle.conflict", { ns: "projects", name });
		default:
			return translate("tiles.diffTitle.changes", { ns: "projects", name });
	}
}

/**
 * The tiles: split, drag and close tabs as in VS Code (Dockview, behind this
 * component). The layout is restored from the server and saved back as it
 * changes; `empty` is what an empty area shows.
 */
export const Tiles = forwardRef<
	TilesHandle,
	{
		/** Where the layout loads from and saves to. */
		store: LayoutStore;
		/** Tabs to open when no layout is saved yet. */
		initialTabs?: Tab[];
		/** Buttons at the end of each tab bar, such as + for a new session. */
		headerActions?: React.FunctionComponent<IDockviewHeaderActionsProps>;
		/** Draws every tab, such as a session's with its state. */
		tabComponent?: React.FunctionComponent<IDockviewPanelHeaderProps>;
		currentProject?: string;
		empty: React.FunctionComponent<IWatermarkPanelProps>;
		/** Defined once, outside render: Dockview reads it when it starts. */
		panels?: Panels;
		/** The focused tab's ID and params. */
		onActive?: (id: string | undefined, params: Record<string, unknown> | undefined) => void;
		/** The open tabs, whenever they change. */
		onTabs?: (tabs: OpenTab[]) => void;
		/** Hides the tab bars (phones list the tabs elsewhere). */
		hideTabs?: boolean;
		/** A tab to open once ready, over any saved layout; then `onOpened`. */
		openOnReady?: Tab;
		onOpened?: () => void;
		/** The projects whose files are read-only. */
		readOnlyProjects?: string[];
		/**
		 * Kinds of window Ahab no longer has: a saved layout's windows of
		 * these kinds are left out (stored layouts stay readable), then
		 * `onRetired` says which were.
		 */
		retired?: readonly string[];
		onRetired?: (kinds: string[]) => void;
	}
>(function Tiles(
	{
		readOnlyProjects,
		store,
		currentProject,
		empty,
		panels,
		initialTabs,
		headerActions,
		onActive,
		onTabs,
		hideTabs,
		openOnReady,
		onOpened,
		tabComponent,
		retired,
		onRetired,
	},
	ref,
) {
	const { t } = useTranslation("projects");
	const components = useMemo(() => ({ ...builtIn, ...panels }), [panels]);
	const readOnlyKey = (readOnlyProjects ?? []).join(",");
	const info = useMemo<TilesInfo>(
		() => ({ readOnly: (id) => readOnlyKey.split(",").includes(id) }),
		[readOnlyKey],
	);
	const api = useRef<DockviewApi | null>(null);
	// Dockview's callbacks are set up once; they use the latest store.
	const latest = useRef(store);
	useEffect(() => {
		latest.current = store;
	}, [store]);
	// Dockview's listeners are set once; they call the latest callbacks.
	const callbacks = useRef({ onActive, onTabs });
	useEffect(() => {
		callbacks.current = { onActive, onTabs };
	}, [onActive, onTabs]);
	const revision = useRef(0);
	// Tabs closed lately, to reopen (⌃⇧T).
	const closed = useRef<Tab[]>([]);
	// Tabs asked for before Dockview is ready, opened once it is.
	const queued = useRef<{ tab: Tab; replace?: string }[]>([]);
	// Reports the open tabs; set once Dockview is ready.
	const reportTabs = useRef<() => void>(() => {});
	const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
	const current = useRef(currentProject);
	useEffect(() => {
		current.current = currentProject;
	}, [currentProject]);

	useImperativeHandle(
		ref,
		() => ({
			openFile(projectId, projectName, path, line, beside, keep) {
				const dock = api.current;
				if (!dock) return;
				const id = fileTabId(projectId, path);

				const elsewhere = projectId !== current.current;
				const group = beside ? dock.activeGroup : undefined;
				openTab(
					dock,
					{
						id,
						component: "file",
						...(group ? { position: { referenceGroup: group, direction: "right" as const } } : {}),
						title: elsewhere
							? translate("tiles.otherProjectTitle", {
									ns: "projects",
									name: externalName(path) ?? baseName(path),
									project: projectName,
								})
							: (externalName(path) ?? baseName(path)),
						params: {
							kind: "file",
							projectId,
							projectName,
							path,
							line,
							jump: line ? Date.now() : undefined,
						} satisfies TabParams,
					},
					{ keep },
				);
			},
			openDiff(tab) {
				const dock = api.current;
				if (!dock) return;
				const id = diffTabId(tab);
				openTab(dock, {
					id,
					component: "diff",
					title: diffTitle(tab),
					params: { kind: "diff", ...tab } satisfies DiffTab,
				});
			},
			open(tab, replace) {
				const dock = api.current;
				if (!dock) {
					queued.current.push({ tab, replace });
					return;
				}
				openTab(dock, tab, { keep: tab.keep, replace });
				reportTabs.current();
			},
			keepActive() {
				const panel = api.current?.activePanel;
				if (panel) keepOpen(panel);
			},
			close(id) {
				api.current?.getPanel(id)?.api.close();
			},
			focus(id) {
				api.current?.getPanel(id)?.api.setActive();
			},
			closeActive() {
				api.current?.activePanel?.api.close();
			},
			reopenClosed() {
				const dock = api.current;
				const tab = closed.current.pop();
				if (!dock || !tab) return;
				const open = dock.getPanel(tab.id);
				if (open) open.api.setActive();
				else openTab(dock, tab, { keep: true });
			},
			cycle(step) {
				const panel = api.current?.activePanel;
				if (!panel) return;
				const siblings = panel.api.group.panels;
				const at = siblings.findIndex((item) => item.id === panel.id);
				siblings[(at + step + siblings.length) % siblings.length]?.api.setActive();
			},
			splitActive(side) {
				const panel = api.current?.activePanel;
				// A tab alone in its tile has nowhere to split from.
				if (!panel || panel.api.group.panels.length < 2) return;
				keepOpen(panel);
				panel.api.moveTo({ group: panel.api.group, position: side });
			},
		}),
		// Built once: it only reads refs, and the workbench keeps it in state.
		[],
	);

	// A change not saved yet is saved as the tiles go, such as a tab closed
	// just before leaving.
	const unsaved = useRef<(() => void) | undefined>(undefined);
	const alive = useRef(false);
	useEffect(() => {
		alive.current = true;
		const online = () => unsaved.current?.();
		window.addEventListener("online", online);
		return () => {
			alive.current = false;
			window.removeEventListener("online", online);
			clearTimeout(timer.current);
			unsaved.current?.();
		};
	}, []);

	if (store.failed)
		return <p className="text-muted-foreground p-6 text-sm">{t("tiles.layoutFailed")}</p>;
	if (store.layout === undefined) return null;
	const saved = store.layout;

	return (
		<TilesContext.Provider value={info}>
			<div
				className="flex h-full flex-col"
				onInput={() => {
					if (api.current?.activePanel) keepOpen(api.current.activePanel);
				}}
				onKeyDownCapture={(event) => {
					if (
						!event.metaKey &&
						!event.ctrlKey &&
						!event.altKey &&
						(event.key.length === 1 ||
							event.key === "Enter" ||
							event.key === "Backspace" ||
							event.key === "Delete")
					) {
						if (api.current?.activePanel) keepOpen(api.current.activePanel);
					}
				}}
			>
				{store.conflict ? (
					<div className="border-border flex flex-wrap items-center gap-2 border-b px-3 py-2 text-sm">
						<p role="status" className="mr-auto">
							{t("tiles.layoutConflict")}
						</p>
						<Button size="sm" variant="outline" onClick={store.conflict.useSaved}>
							{t("tiles.useSavedLayout")}
						</Button>
						<Button size="sm" onClick={store.conflict.keepMine}>
							{t("tiles.keepMyLayout")}
						</Button>
					</div>
				) : null}
				<DockviewReact
					className={hideTabs ? "ahab-tabless min-h-0 flex-1" : "min-h-0 flex-1"}
					theme={theme}
					components={components}
					watermarkComponent={empty}
					rightHeaderActionsComponent={headerActions}
					defaultTabComponent={tabComponent}
					onReady={(event) => {
						api.current = event.api;
						if (saved) {
							revision.current = saved.revision;
							const { layout, removed } = withoutRetired(saved.layout, retired ?? []);
							try {
								event.api.fromJSON(layout as Parameters<DockviewApi["fromJSON"]>[0]);
								// Tiles that held only retired windows go.
								if (removed.length) {
									if (event.api.panels.length === 0) event.api.clear();
									else
										for (const group of event.api.groups.filter((each) => !each.panels.length))
											event.api.removeGroup(group);
								}
							} catch {
								// A layout this version cannot read starts empty.
								event.api.clear();
							}
							if (removed.length) onRetired?.(removed);
						} else {
							for (const tab of initialTabs ?? []) {
								event.api.addPanel({
									id: tab.id,
									component: tab.component,
									title: tab.title,
									params: tab.params,
								});
							}
						}
						const report = () =>
							callbacks.current.onTabs?.(
								event.api.panels.map((panel) => ({
									id: panel.id,
									component: panel.view.contentComponent,
									title: panel.title ?? panel.id,
									params: (panel.params ?? {}) as Record<string, unknown>,
									active: panel.id === event.api.activePanel?.id,
								})),
							);
						report();
						event.api.onDidActivePanelChange(({ panel }) => {
							callbacks.current.onActive?.(
								panel?.id,
								panel?.params as Record<string, unknown> | undefined,
							);
							report();
						});
						event.api.onDidAddPanel(report);
						event.api.onDidRemovePanel((panel) => {
							report();
							if (isPreview(panel.params)) return;
							closed.current.push({
								id: panel.id,
								component: panel.view.contentComponent,
								title: panel.title ?? panel.id,
								params: (panel.params ?? {}) as Record<string, unknown>,
							});
							closed.current = closed.current.slice(-REOPEN_LIMIT);
						});
						reportTabs.current = report;
						event.api.onDidMovePanel(({ panel }) => keepOpen(panel));
						const asked = queued.current.splice(0);
						if (openOnReady) asked.push({ tab: openOnReady });
						for (const { tab, replace } of asked) {
							openTab(event.api, tab, { keep: tab.keep, replace });
						}
						if (openOnReady) onOpened?.();
						const scheduleSave = (restoring = false) => {
							let layout: Record<string, unknown>;
							try {
								layout = event.api.toJSON() as unknown as Record<string, unknown>;
							} catch {
								// Dockview already went away.
								return;
							}
							if (!restoring) latest.current.stage?.(layout, revision.current);
							let attempts = 0;
							const save = () => {
								clearTimeout(timer.current);
								unsaved.current = undefined;
								if (latest.current.conflict) return;
								void latest.current.save(layout, revision.current).then(
									(next) => {
										revision.current = next;
									},
									(error) => {
										if (
											!alive.current ||
											unsaved.current ||
											problemCode(error) === "layout_changed"
										)
											return;
										unsaved.current = save;
										timer.current = setTimeout(save, Math.min(30_000, 2_000 * 2 ** attempts++));
									},
								);
							};
							clearTimeout(timer.current);
							unsaved.current = save;
							timer.current = setTimeout(save, 800);
						};
						event.api.onDidLayoutChange(() => scheduleSave());
						if (store.pending) scheduleSave(true);
					}}
				/>
			</div>
		</TilesContext.Provider>
	);
});
