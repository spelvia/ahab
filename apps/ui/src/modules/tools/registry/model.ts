/**
 * A machine's tools in plain terms: what each state says, which
 * actions a tool offers, how the list is searched, filtered and sorted,
 * and how install jobs from the event stream join the list.
 */

import type {
	MachineToolsDto,
	ToolJobDto,
	ToolStateDto,
	ToolStatusDto,
} from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";

/** A tool's kind as records name it. */
export type ToolKind = "languageServer" | "formatter" | "linter" | "check";

/** The kinds, in the table's order; `kindLabel` names each. */
export const KINDS: ToolKind[] = ["languageServer", "formatter", "linter", "check"];

const isToolKind = (kind: string): kind is ToolKind => (KINDS as string[]).includes(kind);

/** The kind's short word for the table. */
export function kindLabel(kind: string): string {
	return isToolKind(kind) ? t(`tools:kinds.${kind}`) : kind;
}

/** What a tool's state says in its row, with its detail where it has one. */
export function stateWord(tool: ToolStatusDto): string {
	switch (tool.state) {
		case "installed":
			return t("tools:toolState.installed");
		case "found":
			return tool.detail ?? t("tools:toolState.found");
		case "available":
			return t("tools:toolState.available");
		case "missing":
			return tool.origin === "machine"
				? t("tools:toolState.notFound")
				: t("tools:toolState.missing");
		case "cannotRun":
			return tool.reason ?? t("tools:toolState.cannotRun");
		case "off":
			return t("tools:toolState.off");
		default:
			return t("tools:toolState.unknown");
	}
}

/** The dot's colour class for a state. */
export function stateTone(state: ToolStateDto): "ok" | "muted" | "warn" | "off" {
	if (state === "installed" || state === "found") return "ok";
	if (state === "missing" || state === "cannotRun") return "warn";
	if (state === "off") return "off";
	return "muted";
}

/** The filter on states. */
export type StateFilter = "all" | "present" | "available" | "problems" | "off";

/** The filters on states; `tools:stateFilters.<id>` names each. */
export const STATE_FILTERS: StateFilter[] = ["all", "present", "available", "problems", "off"];

function inState(tool: ToolStatusDto, filter: StateFilter): boolean {
	switch (filter) {
		case "all":
			return true;
		case "present":
			return tool.state === "installed" || tool.state === "found";
		case "available":
			return tool.state === "available";
		case "problems":
			return tool.state === "missing" || tool.state === "cannotRun";
		case "off":
			return tool.state === "off";
	}
}

export interface ToolFilter {
	query: string;
	/** A language ID, or empty for all. */
	language: string;
	/** A kind, or empty for all. */
	kind: string;
	state: StateFilter;
}

export const NO_FILTER: ToolFilter = { query: "", language: "", kind: "", state: "all" };

/** Whether a tool passes the search and the filters. */
export function matches(
	tool: ToolStatusDto,
	filter: ToolFilter,
	languageName: (id: string) => string = (id) => id,
): boolean {
	if (filter.language && !tool.languages.includes(filter.language)) return false;
	if (filter.kind && tool.kind !== filter.kind) return false;
	if (!inState(tool, filter.state)) return false;
	const query = filter.query.trim().toLowerCase();
	if (!query) return true;
	return [tool.id, tool.name, tool.description ?? "", ...tool.languages.map(languageName)].some(
		(text) => text.toLowerCase().includes(query),
	);
}

const KIND_ORDER: string[] = KINDS;

/** By language, a language's defaults first, then by kind and name. */
export function sortTools(
	tools: ToolStatusDto[],
	languageName: (id: string) => string = (id) => id,
): ToolStatusDto[] {
	// Tools for no language go last.
	const first = (tool: ToolStatusDto) => {
		const language = tool.languages[0];
		return language ? languageName(language) : "￿";
	};
	return tools.toSorted(
		(a, b) =>
			first(a).localeCompare(first(b)) ||
			Number(Boolean(b.default)) - Number(Boolean(a.default)) ||
			KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind) ||
			a.name.localeCompare(b.name),
	);
}

/** Every language the tools serve, for the filter. */
export function languagesOf(tools: ToolStatusDto[]): string[] {
	return [...new Set(tools.flatMap((tool) => tool.languages))];
}

/** What a tool offers to do on its machine. */
export interface ToolActions {
	install: boolean;
	update: boolean;
	remove: boolean;
}

export function actionsOf(tool: ToolStatusDto): ToolActions {
	return {
		install: tool.state === "available",
		update: tool.state === "installed" && Boolean(tool.updateAvailable),
		remove: tool.state === "installed",
	};
}

/** The tools Update all updates. */
export function updatable(tools: ToolStatusDto[]): ToolStatusDto[] {
	return tools.filter((tool) => actionsOf(tool).update);
}

/** A job as the event stream sends it: with the machine it runs on. */
export type ToolJobEvent = ToolJobDto & { machine?: string };

export function isRunning(job: ToolJobDto | undefined): boolean {
	return job?.state === "running";
}

/** The list with a job's newest state: one job per tool, the newest wins. */
export function withJob(list: MachineToolsDto, job: ToolJobDto): MachineToolsDto {
	const jobs = (list.jobs ?? []).filter((item) => item.tool !== job.tool);
	return { ...list, jobs: [...jobs, job] };
}

/** A tool's job, when one runs or last ended. */
export function jobOf(list: MachineToolsDto | undefined, tool: string): ToolJobDto | undefined {
	return list?.jobs?.find((job) => job.tool === tool);
}

/** A job's one line: `Installing: downloading`, `Install failed at unpacking: …`. */
export function jobLine(job: ToolJobDto): string {
	const action =
		job.action === "install" || job.action === "update" || job.action === "remove"
			? job.action
			: "other";
	const values = { action: job.action, step: job.step, message: job.message };
	if (job.state === "running") return t(`tools:jobs.${action}.running`, values);
	if (job.state === "failed")
		return job.message
			? t(`tools:jobs.${action}.failed`, values)
			: t(`tools:jobs.${action}.failedHere`, values);
	return t(`tools:jobs.${action}.done`, values);
}

/** What a machine's error means for its tools, in a line. */
export function machineProblem(code: string | undefined, message: string): string {
	if (code === "machine_outdated") return t("tools:machineProblem.outdated");
	if (code === "machine_offline") return t("tools:machineProblem.offline");
	if (code === "machine_timeout") return t("tools:machineProblem.timeout");
	return message;
}

/** How long something has run, from its start: `3 min`, `2 h 5 min`, `4 d`. */
export function uptime(startedAt: string, now: number = Date.now()): string {
	const minutes = Math.max(0, Math.floor((now - new Date(startedAt).getTime()) / 60_000));
	if (minutes < 1) return t("tools:uptime.underMinute");
	if (minutes < 60) return t("tools:uptime.minutes", { count: minutes });
	const hours = Math.floor(minutes / 60);
	if (hours < 24)
		return minutes % 60
			? t("tools:uptime.hoursMinutes", { hours, minutes: minutes % 60 })
			: t("tools:uptime.hours", { count: hours });
	return t("tools:uptime.days", { count: Math.floor(hours / 24) });
}
