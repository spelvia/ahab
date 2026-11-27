import type { ViewId } from "@/app/shell/sidebars";
import type { Command, TilesHandle } from "@/modules/projects";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { i18n } from "@/shared/i18n";
import type { ActionId } from "@/shared/keymap/model";
import { mac } from "@/shared/platform";
import { currentFileIcons, setFileIcons } from "@/shared/ui/file-icon";

/** What the palette's commands act on. */
export interface CommandContext {
	project?: Project;
	projects: readonly Project[];
	tiles: TilesHandle | null;
	openProject: (id: string) => void;
	show: (view: ViewId) => void;
	newSession: () => void;
	newTerminal: () => void;
	quickOpen: () => void;
	findInFiles: () => void;
	/** Shows the bottom panel's Problems. */
	problems: () => void;
	/** Opens or closes the bottom panel. */
	togglePanel: () => void;
	/** Shows the bottom panel's Output. */
	output: () => void;
	/** F8, ⇧F8 in the focused file window; none without one. */
	goToProblem?: (direction: 1 | -1) => void;
	switchBranch: () => void;
	runCommand: () => void;
	toggleSidebar: (side: "left" | "right") => void;
	openFolder: () => void;
	settings: (page: "projects" | "machines" | "features" | "keyboard") => void;
	/** Whether a core feature is on. */
	on: (feature: "terminal" | "files" | "git" | "search") => boolean;
	/** An action's key, shown beside it. */
	hint: (action: ActionId) => string | undefined;
}

/** The command palette's list: sessions, projects, views, files, Git, windows, settings. */
export function workbenchCommands(at: CommandContext): Command[] {
	const { tiles, on, hint } = at;
	const t = i18n.getFixedT(null, "shell");
	const list: Command[] = [];
	if (at.project)
		list.push({
			id: "new-session",
			group: t("commands.groups.session"),
			label: t("commands.newSession"),
			hint: hint("session.new"),
			run: at.newSession,
		});
	if (at.project && on("terminal"))
		list.push({
			id: "new-terminal",
			group: t("commands.groups.session"),
			label: t("commands.newTerminal"),
			hint: hint("terminal.new"),
			run: at.newTerminal,
		});
	if (at.project && on("terminal"))
		list.push({
			id: "run-command",
			group: t("commands.groups.session"),
			label: t("commands.runCommand"),
			run: at.runCommand,
		});
	for (const item of at.projects)
		if (item.id !== at.project?.id)
			list.push({
				id: `project:${item.id}`,
				group: t("commands.groups.project"),
				label: item.name,
				hint: `${item.machineName} · ${item.path}`,
				run: () => at.openProject(item.id),
			});
	list.push({
		id: "sessions",
		group: t("commands.groups.view"),
		label: t("commands.sessions"),
		run: () => at.show("sessions"),
	});
	if (at.project)
		list.push(
			{
				id: "problems",
				group: t("commands.groups.view"),
				label: t("commands.showProblems"),
				hint: hint("panel.problems"),
				run: at.problems,
			},
			{
				id: "output",
				group: t("commands.groups.view"),
				label: t("commands.showOutput"),
				hint: hint("panel.output"),
				run: at.output,
			},
			...(at.goToProblem
				? [
						{
							id: "next-problem",
							group: t("commands.groups.files"),
							label: t("commands.nextProblem"),
							hint: "F8",
							run: () => at.goToProblem?.(1),
						},
						{
							id: "previous-problem",
							group: t("commands.groups.files"),
							label: t("commands.previousProblem"),
							hint: mac ? "⇧F8" : "Shift+F8",
							run: () => at.goToProblem?.(-1),
						},
					]
				: []),
			{
				id: "toggle-panel",
				group: t("commands.groups.view"),
				label: t("commands.togglePanel"),
				hint: hint("panel.toggle"),
				run: at.togglePanel,
			},
		);
	if (on("files"))
		list.push(
			{
				id: "files",
				group: t("commands.groups.view"),
				label: t("commands.files"),
				run: () => at.show("files"),
			},
			{
				id: "go-to-file",
				group: t("commands.groups.files"),
				label: t("commands.goToFile"),
				hint: hint("files.quickOpen"),
				run: at.quickOpen,
			},
		);
	if (on("git"))
		list.push(
			{ id: "git", group: t("commands.groups.view"), label: "Git", run: () => at.show("git") },
			{
				id: "switch-branch",
				group: "Git",
				label: t("commands.switchBranch"),
				run: at.switchBranch,
			},
		);
	if (on("search"))
		list.push(
			{
				id: "search",
				group: t("commands.groups.view"),
				label: t("commands.search"),
				run: () => at.show("search"),
			},
			{
				id: "find-in-files",
				group: t("commands.groups.files"),
				label: t("commands.findInFiles"),
				hint: hint("search.find"),
				run: at.findInFiles,
			},
		);
	list.push(
		{
			id: "file-icons",
			group: t("commands.groups.view"),
			label:
				currentFileIcons() === "material" ? t("commands.plainIcons") : t("commands.materialIcons"),
			run: () => setFileIcons(currentFileIcons() === "material" ? "minimal" : "material"),
		},
		{
			id: "close-tab",
			group: t("commands.groups.window"),
			label: t("commands.closeTab"),
			hint: hint("window.close"),
			run: () => tiles?.closeActive(),
		},
		{
			id: "reopen-tab",
			group: t("commands.groups.window"),
			label: t("commands.reopenTab"),
			hint: hint("window.reopen"),
			run: () => tiles?.reopenClosed(),
		},
		{
			id: "next-tab",
			group: t("commands.groups.window"),
			label: t("commands.nextTab"),
			hint: hint("window.next"),
			run: () => tiles?.cycle(1),
		},
		{
			id: "previous-tab",
			group: t("commands.groups.window"),
			label: t("commands.previousTab"),
			hint: hint("window.previous"),
			run: () => tiles?.cycle(-1),
		},
		{
			id: "split-tab",
			group: t("commands.groups.window"),
			label: t("commands.splitTab"),
			hint: hint("window.split"),
			run: () => tiles?.splitActive("right"),
		},
		{
			id: "toggle-left",
			group: t("commands.groups.view"),
			label: t("commands.toggleLeft"),
			hint: hint("sidebar.left"),
			run: () => at.toggleSidebar("left"),
		},
		{
			id: "toggle-right",
			group: t("commands.groups.view"),
			label: t("commands.toggleRight"),
			hint: hint("sidebar.right"),
			run: () => at.toggleSidebar("right"),
		},
		{
			id: "open",
			group: t("commands.groups.action"),
			label: t("commands.openFolder"),
			run: at.openFolder,
		},
		{
			id: "manage",
			group: t("commands.groups.settings"),
			label: t("commands.manageProjects"),
			run: () => at.settings("projects"),
		},
		{
			id: "machines",
			group: t("commands.groups.settings"),
			label: t("commands.machines"),
			run: () => at.settings("machines"),
		},
		{
			id: "features",
			group: t("commands.groups.settings"),
			label: t("commands.features"),
			run: () => at.settings("features"),
		},
		{
			id: "keyboard",
			group: t("commands.groups.settings"),
			label: t("commands.keyboard"),
			run: () => at.settings("keyboard"),
		},
	);
	return list;
}
