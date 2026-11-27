import type { DockviewApi, IDockviewPanel } from "dockview-react";

/** What a tile's tab shows. Sessions and terminals add their kinds later. */
interface FileTab {
	kind: "file";
	projectId: string;
	/** Shown in the tab when it is not the current project. */
	projectName: string;
	/** Relative to the project. */
	path: string;
	/** A heading to show (markdown), when a link opened it there. */
	anchor?: string;
	/** A line to show, when search or a link asked for one. */
	line?: number;
	/** Changes with each request to go to the anchor or line. */
	jump?: number;
}

/** A diff window: a changed file's two sides. */
export interface DiffTab {
	kind: "diff";
	projectId: string;
	/** Relative to the repository. */
	path: string;
	oldPath?: string;
	group: "changes" | "staged" | "untracked" | "conflicts" | "commit";
	commit?: string;
	/** The project's folder within the repository, to open the file. */
	prefix: string;
	/** A repository inside the project's folder; absent for the one it is in. */
	repository?: string;
}

export type TabParams = FileTab;

/** A stable panel ID per diff. */
export function diffTabId(tab: Omit<DiffTab, "kind" | "prefix">): string {
	return `diff:${tab.projectId}:${tab.repository ?? ""}:${tab.group}:${tab.commit ?? ""}:${tab.path}`;
}

/** A stable panel ID per file, so opening it twice focuses the open tab. */
export function fileTabId(projectId: string, path: string): string {
	return `file:${projectId}:${path}`;
}

/** The last part of a path. */
export function baseName(path: string): string {
	const parts = path.split("/").filter(Boolean);
	return parts.at(-1) ?? path;
}

/** The parts of a relative path, for a breadcrumb. */
export function pathParts(path: string): string[] {
	return path.split("/").filter(Boolean);
}

/** Preview state belongs to the shell, outside a window's own parameters. */
export function isPreview(params: Record<string, unknown> | undefined): boolean {
	return params?.ahabPreview === true;
}

/** Keep a preview when explicitly requested, edited, or moved. */
export function keepOpen(panel: Pick<IDockviewPanel, "api" | "params">): void {
	if (isPreview(panel.params)) panel.api.updateParameters({ ahabPreview: false });
}

/** Open at most one replaceable preview per group, regardless of its content. */
export function openTab(
	dock: DockviewApi,
	tab: Parameters<DockviewApi["addPanel"]>[0],
	options: { keep?: boolean; replace?: string } = {},
): IDockviewPanel {
	const existing = dock.getPanel(tab.id);
	if (existing) {
		if (tab.title !== undefined) existing.api.setTitle(tab.title);
		if (tab.params) existing.api.updateParameters(tab.params);
		if (options.keep) keepOpen(existing);
		existing.api.setActive();
		return existing;
	}
	const replaced = options.replace ? dock.getPanel(options.replace) : undefined;
	// A split explicitly requests another pane; preserve the source preview.
	const splitting = tab.position && tab.position.direction !== "within";
	if (splitting && dock.activePanel) keepOpen(dock.activePanel);
	const group = replaced?.group ?? (!splitting ? dock.activeGroup : undefined);
	const preview = group?.panels.find((panel) => isPreview(panel.params));
	const previous = replaced ?? (!options.keep ? preview : undefined);
	const params = { ...tab.params, ahabPreview: !options.keep };
	const panel = previous
		? dock.addPanel({
				...tab,
				params,
				floating: false,
				position: {
					referencePanel: previous,
					direction: "within",
					index: previous.group.panels.findIndex((item) => item.id === previous.id),
				},
			})
		: dock.addPanel({ ...tab, params });
	// Add first so replacing a group's only tab preserves the pane and its size.
	previous?.api.close();
	return panel;
}

/**
 * A saved layout without the windows of kinds Ahab no longer has (such as
 * `problems`, now the bottom panel's): their panels and their places in
 * tab groups go, each group's tab in front staying one it still holds.
 * What else the layout holds is kept as it is; one Ahab cannot read is
 * given back untouched. Answers the kinds it took out.
 */
export function withoutRetired(
	layout: unknown,
	retired: readonly string[],
): { layout: unknown; removed: string[] } {
	if (!retired.length || !isObject(layout) || !isObject(layout.panels))
		return { layout, removed: [] };
	const gone = new Set<string>();
	const removed = new Set<string>();
	const panels: Record<string, unknown> = {};
	for (const [id, panel] of Object.entries(layout.panels)) {
		const kind = isObject(panel) ? panel.contentComponent : undefined;
		if (typeof kind === "string" && retired.includes(kind)) {
			gone.add(id);
			removed.add(kind);
		} else panels[id] = panel;
	}
	if (!gone.size) return { layout, removed: [] };
	const clean = (node: unknown): unknown => {
		if (!isObject(node)) return node;
		if (node.type === "branch" && Array.isArray(node.data))
			return { ...node, data: node.data.map(clean) };
		if (node.type === "leaf" && isObject(node.data) && Array.isArray(node.data.views)) {
			const views = node.data.views.filter((view) => !gone.has(String(view)));
			const active = views.includes(node.data.activeView) ? node.data.activeView : views[0];
			return { ...node, data: { ...node.data, views, activeView: active } };
		}
		return node;
	};
	const grid = isObject(layout.grid)
		? { ...layout.grid, root: clean(layout.grid.root) }
		: layout.grid;
	return { layout: { ...layout, grid, panels }, removed: [...removed] };
}

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
