import type { Project } from "@/shared/api/generated/ahab.schemas";
import { t as translate } from "@/shared/i18n";

/** A project's state, in a few words, and its dot. */
export function projectState(project: Project): { label: string; dot: string } {
	switch (project.availability) {
		case "available":
			return { label: project.machineName, dot: "bg-success" };
		case "machineOffline":
			return {
				label: translate("projectState.offline", {
					ns: "projects",
					machine: project.machineName,
				}),
				dot: "border border-muted-foreground",
			};
		case "missing":
			return {
				label: translate("projectState.missing", {
					ns: "projects",
					machine: project.machineName,
				}),
				dot: "bg-destructive",
			};
	}
}

/** One group of projects: its heading (none for the ungrouped ones). */
export interface ProjectGroup<P> {
	group: string | null;
	projects: P[];
}

/**
 * Projects under their group headings: groups in the order they first
 * appear, the ungrouped ones last; each group keeps the given order.
 */
export function groupProjects<P extends Pick<Project, "group">>(projects: P[]): ProjectGroup<P>[] {
	const groups = new Map<string | null, P[]>();
	for (const project of projects) {
		const group = project.group?.trim() || null;
		if (group !== null && !groups.has(group)) groups.set(group, []);
	}
	groups.set(null, []);
	for (const project of projects) groups.get(project.group?.trim() || null)?.push(project);
	return [...groups]
		.map(([group, members]) => ({ group, projects: members }))
		.filter((entry) => entry.projects.length > 0);
}

/** The IDs of projects whose name another listed project shares. */
export function sharedNames(projects: Pick<Project, "id" | "name">[]): Set<string> {
	const counts = new Map<string, number>();
	for (const project of projects) counts.set(project.name, (counts.get(project.name) ?? 0) + 1);
	return new Set(
		projects.filter((project) => (counts.get(project.name) ?? 0) > 1).map((project) => project.id),
	);
}

/** Where a project is: `folder on machine`, to tell same-named ones apart. */
export function projectPlace(project: Pick<Project, "path" | "machineName">): string {
	return translate("projectPlace", {
		ns: "projects",
		path: project.path,
		machine: project.machineName,
	});
}

/**
 * The projects whose files are read-only, for a project's windows: file
 * tabs can show other projects' files, each as its own project allows.
 * `current` is the freshest word on its own mode.
 */
export function readOnlyProjects(
	current: Pick<Project, "id" | "mode">,
	projects: readonly Pick<Project, "id" | "mode">[] = [],
): string[] {
	const others = projects.filter(
		(project) => project.id !== current.id && project.mode === "readOnly",
	);
	return [...(current.mode === "readOnly" ? [current.id] : []), ...others.map((p) => p.id)];
}

/**
 * Projects in the order people see them: group by group, as the switcher
 * and Settings → Projects list them. Alt+1…9 follow this order.
 */
export function inListOrder<P extends Pick<Project, "group">>(projects: P[]): P[] {
	return groupProjects(projects).flatMap((entry) => entry.projects);
}

/**
 * Every project's ID in list order, after moving `id` to where `target` is
 * (or one step, with a number), within its own group: a project changes its
 * group on its page, not by being dragged. `undefined` when nothing moves.
 */
export function reordered<P extends Pick<Project, "id" | "group">>(
	projects: P[],
	id: string,
	target: string | -1 | 1,
): string[] | undefined {
	const list = inListOrder(projects);
	const from = list.findIndex((project) => project.id === id);
	if (from < 0) return undefined;
	const group = (project: P | undefined) => project?.group?.trim() || null;
	const to =
		typeof target === "number" ? from + target : list.findIndex((project) => project.id === target);
	if (to < 0 || to >= list.length || to === from) return undefined;
	if (group(list[to]) !== group(list[from])) return undefined;
	const ids = list.map((project) => project.id);
	ids.splice(from, 1);
	ids.splice(to, 0, id);
	return ids;
}
