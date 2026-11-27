import { cn } from "cn";
import { ArrowLeftRight, Bot, Files, GitBranch, Search } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";

import type { PluginViewItem } from "@/modules/plugins";
import { i18n, useTranslation } from "@/shared/i18n";
import {
	rememberSidebars,
	type Sidebar,
	type Sidebars,
	sidebars as savedSidebars,
} from "@/shared/preferences/store";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";

export type Side = "left" | "right";
/** Ahab's own views. */
type OwnView = "sessions" | "files" | "git" | "search";
/** A view: Ahab's own, or a plugin's (`plugin:<plugin>:<id>`). */
export type ViewId = OwnView | (string & {});

/** Ahab's own views, and how their icons look; `label` is the key of its name. */
export const VIEWS: Record<OwnView, { label: `views.${OwnView}`; icon: React.ReactNode }> = {
	sessions: { label: "views.sessions", icon: <Bot /> },
	files: { label: "views.files", icon: <Files /> },
	git: { label: "views.git", icon: <GitBranch /> },
	search: { label: "views.search", icon: <Search /> },
};
const ALL = Object.keys(VIEWS) as ViewId[];

const NONE: readonly never[] = [];

/** Whether `view` is a plugin's: kept in layouts even while its plugin is not loaded. */
const isPlugins = (view: string) => view.startsWith("plugin:");

/** A view's label: Ahab's own, or what its plugin calls it. */
export function viewLabel(view: string, plugins: readonly PluginViewItem[]): string {
	const own = VIEWS[view as OwnView];
	if (own) return i18n.t(own.label, { ns: "shell" });
	return plugins.find((item) => item.key === view)?.title ?? view;
}

const MIN_WIDTH = 200;
const MAX_WIDTH = 560;

/** Sessions on the left; the session's context (files, changes, search) on the right. */
const DEFAULT: Sidebars = {
	left: { views: ["sessions"], active: "sessions", open: true, width: 288 },
	right: { views: ["files", "git", "search"], active: "files", open: true, width: 300 },
};

/** A saved layout made whole: every view on exactly one side, widths in bounds. */
function normalise(layout: Sidebars | undefined): Sidebars {
	if (!layout) return DEFAULT;
	const seen = new Set<string>();
	const side = (bar: Sidebar | undefined, fallback: Sidebar): Sidebar => {
		const views = (bar?.views ?? fallback.views).filter(
			(view) => (ALL.includes(view) || isPlugins(view)) && !seen.has(view) && seen.add(view),
		);
		return {
			views,
			active: views.includes(bar?.active ?? "") ? bar?.active : views[0],
			open: bar?.open ?? fallback.open,
			width: Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, bar?.width ?? fallback.width)),
		};
	};
	const left = side(layout.left, DEFAULT.left);
	const right = side(layout.right, DEFAULT.right);
	// A view new since the layout was saved goes where it goes by default.
	for (const view of ALL)
		if (!seen.has(view)) (DEFAULT.left.views.includes(view) ? left : right).views.push(view);
	return { left, right };
}

const other = (side: Side): Side => (side === "left" ? "right" : "left");

/**
 * The two sidebars: which views each holds, which is shown, open or closed,
 * width. Plugins' views (`plugins`, their keys) new to the layout go on the
 * right.
 */
export function useSidebars(plugins: readonly string[] = NONE, hidden: readonly string[] = NONE) {
	const [stored, setStored] = useState(() => normalise(savedSidebars()));
	const joined = plugins.join("\n");
	// Plugins' views not yet in the layout join the right sidebar.
	const withPlugins = useCallback(
		(current: Sidebars): Sidebars => {
			const keys = joined ? joined.split("\n") : [];
			const missing = keys.filter(
				(key) => !current.left.views.includes(key) && !current.right.views.includes(key),
			);
			if (!missing.length) return current;
			return {
				...current,
				right: {
					...current.right,
					views: [...current.right.views, ...missing],
					active: current.right.active ?? missing[0],
				},
			};
		},
		[joined],
	);
	// Views of core features switched off keep their place, unseen.
	const hiddenKey = hidden.join("\n");
	const layout = useMemo(() => {
		const full = withPlugins(stored);
		if (!hiddenKey) return full;
		const off = new Set(hiddenKey.split("\n"));
		const visible = (bar: Sidebar): Sidebar => {
			const views = bar.views.filter((view) => !off.has(view));
			const active = bar.active && !off.has(bar.active) ? bar.active : views[0];
			return { ...bar, views, active, open: bar.open && views.length > 0 };
		};
		return { left: visible(full.left), right: visible(full.right) };
	}, [stored, withPlugins, hiddenKey]);
	const update = useCallback(
		(change: (current: Sidebars) => Sidebars) => {
			setStored((current) => {
				const next = change(withPlugins(current));
				rememberSidebars(next);
				return next;
			});
		},
		[withPlugins],
	);

	/** The side a view is on. */
	const sideOf = useCallback(
		(view: ViewId): Side => (layout.left.views.includes(view) ? "left" : "right"),
		[layout],
	);

	/** Shows a view, opening its sidebar. */
	const show = useCallback(
		(view: ViewId) =>
			update((current) => {
				const side: Side = current.left.views.includes(view) ? "left" : "right";
				return { ...current, [side]: { ...current[side], active: view, open: true } };
			}),
		[update],
	);

	const toggle = useCallback(
		(side: Side) =>
			update((current) => ({
				...current,
				[side]: { ...current[side], open: !current[side].open },
			})),
		[update],
	);

	/** Moves a view to a side, before `before` if given, and shows it there. */
	const move = useCallback(
		(view: ViewId, side: Side, before?: ViewId) =>
			update((current) => {
				const from = other(side);
				const without = (bar: Sidebar) => bar.views.filter((item) => item !== view);
				const target = without(current[side]);
				const at = before ? target.indexOf(before) : -1;
				target.splice(at === -1 ? target.length : at, 0, view);
				const left = without(current[from]);
				return {
					[from]: {
						...current[from],
						views: left,
						active: current[from].active === view ? left[0] : current[from].active,
					},
					[side]: { ...current[side], views: target, active: view, open: true },
				} as unknown as Sidebars;
			}),
		[update],
	);

	const resize = useCallback(
		(side: Side, width: number) =>
			update((current) => ({
				...current,
				[side]: {
					...current[side],
					width: Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(width))),
				},
			})),
		[update],
	);

	return { layout, sideOf, show, toggle, move, resize };
}

/** The drag payload's type: a view's ID. */
const DRAGGED = "application/x-ahab-view";

/**
 * A column of view icons at the outer edge of a side. Clicking shows the
 * view, or hides the sidebar when it is the view shown; dragging an icon to the other bar moves it there, as does its
 * right-click menu.
 */
export function ActivityBar({
	side,
	bar,
	badges,
	counts,
	onShow,
	onHide,
	onMove,
	plugins = NONE,
	onOpenWindow,
}: {
	side: Side;
	bar: Sidebar;
	/** Plugins' views and window icons. */
	plugins?: readonly PluginViewItem[];
	/** A plugin's icon that opens a window was clicked. */
	onOpenWindow?: (item: PluginViewItem, keep?: boolean) => void;
	/** Counts on icons that ask for attention, such as sessions that need you. */
	badges?: Partial<Record<OwnView, number>>;
	/** Quiet counts on icons, such as changed files. */
	counts?: Partial<Record<OwnView, number>>;
	onShow: (view: ViewId) => void;
	/** The shown view's icon was clicked: its sidebar closes. */
	onHide: () => void;
	onMove: (view: ViewId, side: Side, before?: ViewId) => void;
}) {
	const { t } = useTranslation("shell");
	const [over, setOver] = useState(false);
	return (
		<nav
			aria-label={side === "left" ? t("views.left") : t("views.right")}
			onDragOver={(event) => {
				if (!event.dataTransfer.types.includes(DRAGGED)) return;
				event.preventDefault();
				setOver(true);
			}}
			onDragLeave={() => setOver(false)}
			onDrop={(event) => {
				const view = event.dataTransfer.getData(DRAGGED) as ViewId;
				setOver(false);
				if (!ALL.includes(view) && !isPlugins(view)) return;
				event.preventDefault();
				// Dropped on an icon: before it; else at the end.
				const target = (event.target as HTMLElement).closest<HTMLElement>("[data-view]");
				const before = target?.dataset.view as ViewId | undefined;
				onMove(view, side, before === view ? undefined : before);
			}}
			className={cn(
				// Clipped: the active view's mark sits at its outer edge, and
				// must not push the page wider than the window.
				"bg-muted/40 border-border flex w-12 shrink-0 flex-col items-center gap-1 overflow-x-clip py-2",
				side === "left" ? "border-r" : "border-l",
				over && "bg-accent/40",
			)}
		>
			{bar.views.map((id) => {
				const plugin = plugins.find((item) => item.key === id);
				const own = VIEWS[id as OwnView];
				const view = own
					? { label: t(own.label), icon: own.icon }
					: plugin
						? { label: plugin.title, icon: plugin.icon }
						: null;
				// A plugin's view whose plugin is not loaded keeps its place, unseen.
				if (!view) return null;
				const opens = plugin?.opens;
				const active = !opens && bar.open && bar.active === id;
				const badge = badges?.[id as OwnView];
				const count = counts?.[id as OwnView];
				return (
					<ContextMenu key={id}>
						<ContextMenuTrigger asChild>
							<button
								type="button"
								data-view={id}
								draggable
								onDragStart={(event) => {
									event.dataTransfer.setData(DRAGGED, id);
									event.dataTransfer.effectAllowed = "move";
								}}
								onDoubleClick={() => {
									if (opens && plugin) onOpenWindow?.(plugin, true);
								}}
								onClick={(event) =>
									event.detail > 1
										? undefined
										: opens && plugin
											? onOpenWindow?.(plugin)
											: active
												? onHide()
												: onShow(id as ViewId)
								}
								aria-label={
									badge ? t("views.needYou", { label: view.label, count: badge }) : view.label
								}
								aria-pressed={active}
								title={
									active
										? t("views.hide", { label: view.label })
										: t("views.show", { label: view.label })
								}
								className={cn(
									"focus-visible:ring-ring relative grid size-10 place-items-center rounded-lg outline-none focus-visible:ring-2 [&_svg]:size-5",
									active
										? "text-heading before:bg-heading before:absolute before:inset-y-2.5 before:w-0.5 before:rounded-full"
										: "text-muted-foreground hover:text-foreground",
									side === "left" ? "before:-left-1" : "before:-right-1",
								)}
							>
								{view.icon}
								{badge ? (
									<span className="bg-warning absolute top-1 right-1 grid min-w-4 place-items-center rounded-full px-1 text-[0.625rem] leading-4 font-bold text-[#2a1d05]">
										{badge}
									</span>
								) : count ? (
									<span className="bg-muted-foreground/80 text-background absolute top-1 right-0.5 grid min-w-4 place-items-center rounded-full px-1 text-[0.5625rem] leading-4 font-bold">
										{count > 99 ? "99+" : count}
									</span>
								) : null}
							</button>
						</ContextMenuTrigger>
						<ContextMenuContent>
							<ContextMenuItem onSelect={() => onMove(id as ViewId, other(side))}>
								<ArrowLeftRight />
								{other(side) === "left" ? t("views.moveLeft") : t("views.moveRight")}
							</ContextMenuItem>
						</ContextMenuContent>
					</ContextMenu>
				);
			})}
		</nav>
	);
}

/** A sidebar's panel, resized by dragging its inner edge. */
export function SidePanel({
	side,
	width,
	label,
	onResize,
	children,
}: {
	side: Side;
	width: number;
	label: string;
	onResize: (width: number) => void;
	children: React.ReactNode;
}) {
	const { t } = useTranslation("shell");
	const start = useRef<{ x: number; width: number } | null>(null);
	const [dragging, setDragging] = useState<number | null>(null);
	const shown = dragging ?? width;
	return (
		<aside
			aria-label={label}
			style={{ width: shown }}
			className={cn(
				"bg-background border-border relative flex shrink-0 flex-col",
				side === "left" ? "border-r" : "border-l",
			)}
		>
			{children}
			<div
				role="separator"
				aria-orientation="vertical"
				aria-label={side === "left" ? t("views.resizeLeft") : t("views.resizeRight")}
				onPointerDown={(event) => {
					event.currentTarget.setPointerCapture(event.pointerId);
					start.current = { x: event.clientX, width: shown };
				}}
				onPointerMove={(event) => {
					if (!start.current) return;
					const delta = event.clientX - start.current.x;
					const next = start.current.width + (side === "left" ? delta : -delta);
					setDragging(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next)));
				}}
				onPointerUp={() => {
					start.current = null;
					if (dragging !== null) onResize(dragging);
					setDragging(null);
				}}
				className={cn(
					"hover:bg-ring/50 absolute inset-y-0 z-10 w-1 cursor-col-resize transition-colors",
					side === "left" ? "-right-0.5" : "-left-0.5",
					dragging !== null && "bg-ring/60",
				)}
			/>
		</aside>
	);
}
