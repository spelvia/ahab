import type { Session, Project } from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";

/** Windows' folders use `\` when the project's own path does (Windows). */
function separator(root: string): string {
	return /^[A-Za-z]:\\/.test(root) || root.startsWith("\\\\") ? "\\" : "/";
}

/**
 * Where a project's file is on its machine: the project's folder (as
 * the server knows it) joined with the file's path in it.
 */
export function absolutePath(root: string, relative: string): string {
	const sep = separator(root);
	const base = root.endsWith(sep) ? root.slice(0, -1) : root;
	const rest = relative.replace(/^\/+/, "");
	if (!rest) return root;
	return `${base}${sep}${sep === "\\" ? rest.replaceAll("/", "\\") : rest}`;
}

/** `what — machine`, as VS Code's tab tooltips show a remote's name. */
function onMachine(what: string, project?: Pick<Project, "machineName">): string {
	return project?.machineName
		? t("tabTooltip.onMachine", { ns: "shell", what, machine: project.machineName })
		: what;
}

/**
 * A file window's tooltip: its path on its machine and the machine, when
 * its project is known; else its path in the project.
 */
export function fileTooltip(
	path: string,
	project?: Pick<Project, "path" | "machineName">,
	externalName?: string,
): string {
	if (path.startsWith("external:"))
		return onMachine(
			externalName
				? t("tabTooltip.external", { ns: "shell", name: externalName })
				: t("tabTooltip.externalUnnamed", { ns: "shell" }),
			project,
		);
	if (!project) return path;
	return onMachine(absolutePath(project.path, path), project);
}

/** A session's tooltip: its title, its agent (or its command) and where. */
export function sessionTooltip(
	session: Pick<Session, "title" | "kind" | "chats" | "terminal">,
	project?: Pick<Project, "path" | "machineName">,
	title?: string,
): string {
	if (session.kind === "terminal") {
		const command = session.terminal?.command || t("tabTooltip.shell", { ns: "shell" });
		const named = session.terminal?.title;
		const path = project?.path ?? "";
		const what = project
			? named
				? t("tabTooltip.terminalTitledIn", { ns: "shell", command, title: named, path })
				: t("tabTooltip.terminalIn", { ns: "shell", command, path })
			: named
				? t("tabTooltip.terminalTitled", { ns: "shell", command, title: named })
				: command;
		return onMachine(what, project);
	}
	const chat = session.chats[0];
	const parts = [title || session.title, chat?.agent].filter(Boolean);
	return onMachine(parts.join(" · "), project);
}
