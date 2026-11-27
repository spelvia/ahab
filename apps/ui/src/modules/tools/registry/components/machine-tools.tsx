import { cn } from "cn";
import { RotateCw, Search } from "lucide-react";
import { Fragment, type ReactNode, useMemo, useState } from "react";

import {
	useInstallTool,
	useMachineTools,
	useRemoveTool,
	useRescanTools,
	useUpdateTool,
} from "@/modules/tools/registry/api";
import { languageNamer } from "@/modules/tools/registry/components/languages";
import { ToolJob } from "@/modules/tools/registry/components/tool-job";
import {
	actionsOf,
	isRunning,
	jobOf,
	KINDS,
	kindLabel,
	languagesOf,
	machineProblem,
	matches,
	NO_FILTER,
	sortTools,
	STATE_FILTERS,
	type StateFilter,
	stateTone,
	stateWord,
	type ToolFilter,
	updatable,
} from "@/modules/tools/registry/model";
import { useToolJobs } from "@/modules/tools/registry/stream";
import { problemCode } from "@/shared/api/fetch";
import type { Machine, RuntimeDto, ToolStatusDto } from "@/shared/api/generated/ahab.schemas";
import { formatList, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { toastError } from "@/shared/ui/toast";

const DOT = {
	ok: "bg-success",
	warn: "bg-warning",
	muted: "border border-muted-foreground/50",
	off: "bg-muted-foreground/40",
} as const;

const SELECT =
	"border-border bg-background focus-visible:ring-ring h-8 rounded-md border px-2 text-sm outline-none focus-visible:ring-2";

/**
 * Every tool record with its state on a machine, like Mason: its runtimes,
 * a search and filters, and per tool Install, Update or Remove, with each
 * job's progress as it runs. `onOpen` opens a tool's settings; `actions`
 * sit beside Scan again.
 */
export function MachineTools({
	machine,
	onOpen,
	actions,
}: {
	machine: Machine;
	onOpen?: (tool: ToolStatusDto) => void;
	actions?: ReactNode;
}) {
	const tools = useMachineTools(machine.id);
	const rescan = useRescanTools(machine.id);
	const update = useUpdateTool(machine.id);
	useToolJobs(machine.id, machine.online);
	const [filter, setFilter] = useState<ToolFilter>(NO_FILTER);
	const { t } = useTranslation(["tools", "common"]);
	const nameOf = useMemo(() => languageNamer(), []);

	const items = tools.data?.tools ?? [];
	const shown = sortTools(
		items.filter((tool) => matches(tool, filter, nameOf)),
		nameOf,
	);
	const toUpdate = updatable(items).filter((tool) => !isRunning(jobOf(tools.data, tool.id)));
	const set = (patch: Partial<ToolFilter>) => setFilter((current) => ({ ...current, ...patch }));
	const error = tools.error ?? rescan.error;

	return (
		<div className="space-y-4">
			<div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
				<span className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
					{t("machineTools.runtimes")}
				</span>
				{(tools.data?.runtimes ?? []).map((runtime) => (
					<Runtime key={runtime.id} runtime={runtime} />
				))}
				{tools.data && tools.data.runtimes.length === 0 && (
					<span className="text-muted-foreground">{t("machineTools.noRuntimes")}</span>
				)}
				<span className="ml-auto flex items-center gap-1.5">
					<Button
						variant="ghost"
						size="xs"
						disabled={!machine.online || rescan.isPending}
						title={machine.online ? undefined : t("machineProblem.offline")}
						onClick={() => rescan.mutate()}
					>
						<RotateCw className={rescan.isPending ? "animate-spin" : undefined} />
						{rescan.isPending ? t("machineTools.scanning") : t("machineTools.scanAgain")}
					</Button>
					{actions}
				</span>
			</div>

			<div className="flex flex-wrap items-center gap-2">
				<label className="border-border focus-within:ring-ring flex h-8 min-w-48 flex-1 items-center gap-2 rounded-md border px-2.5 focus-within:ring-2 sm:max-w-72">
					<Search className="text-muted-foreground size-3.5" />
					<span className="sr-only">{t("machineTools.search")}</span>
					<input
						value={filter.query}
						onChange={(event) => set({ query: event.target.value })}
						placeholder={t("machineTools.search")}
						className="placeholder:text-muted-foreground min-w-0 flex-1 bg-transparent text-sm outline-none"
					/>
				</label>
				<select
					aria-label={t("machineTools.columns.language")}
					value={filter.language}
					onChange={(event) => set({ language: event.target.value })}
					className={SELECT}
				>
					<option value="">{t("machineTools.allLanguages")}</option>
					{languagesOf(items)
						.toSorted((a, b) => nameOf(a).localeCompare(nameOf(b)))
						.map((id) => (
							<option key={id} value={id}>
								{nameOf(id)}
							</option>
						))}
				</select>
				<select
					aria-label={t("machineTools.columns.kind")}
					value={filter.kind}
					onChange={(event) => set({ kind: event.target.value })}
					className={SELECT}
				>
					<option value="">{t("machineTools.allKinds")}</option>
					{KINDS.map((kind) => (
						<option key={kind} value={kind}>
							{kindLabel(kind)}
						</option>
					))}
				</select>
				<select
					aria-label={t("machineTools.columns.state")}
					value={filter.state}
					onChange={(event) => set({ state: event.target.value as StateFilter })}
					className={SELECT}
				>
					{STATE_FILTERS.map((state) => (
						<option key={state} value={state}>
							{t(`stateFilters.${state}`)}
						</option>
					))}
				</select>
				{toUpdate.length > 0 && (
					<Button
						variant="outline"
						size="sm"
						className="ml-auto"
						disabled={!machine.online}
						onClick={() => {
							for (const tool of toUpdate)
								update.mutate({ id: machine.id, tool: tool.id }, { onError: toastError });
						}}
					>
						{t("machineTools.updateAll", { count: toUpdate.length })}
					</Button>
				)}
			</div>

			{error && (
				<p role="alert" className="text-destructive text-sm">
					{machineProblem(problemCode(error), error.message)}
				</p>
			)}
			{tools.isPending && (
				<p className="text-muted-foreground py-2 text-sm" role="status">
					{t("machineTools.loading")}
				</p>
			)}
			{tools.data && shown.length === 0 && (
				<p className="border-border text-muted-foreground border-y py-3 text-sm">
					{filter.query
						? t("machineTools.noMatchQuery", { query: filter.query })
						: t("machineTools.noMatch")}
				</p>
			)}

			{shown.length > 0 && (
				<div className="overflow-x-auto">
					<table className="w-full text-sm">
						<thead>
							<tr className="text-muted-foreground border-border border-b text-left text-xs">
								<th className="py-1.5 pr-3 font-medium">{t("machineTools.columns.language")}</th>
								<th className="py-1.5 pr-3 font-medium">{t("machineTools.columns.tool")}</th>
								<th className="py-1.5 pr-3 font-medium">{t("machineTools.columns.kind")}</th>
								<th className="py-1.5 pr-3 font-medium">{t("machineTools.columns.state")}</th>
								<th className="py-1.5 pr-3 font-medium">{t("machineTools.columns.version")}</th>
								<th className="py-1.5 font-medium">
									<span className="sr-only">{t("machineTools.columns.actions")}</span>
								</th>
							</tr>
						</thead>
						<tbody className="divide-border divide-y">
							{shown.map((tool) => (
								<Row
									key={tool.id}
									tool={tool}
									machine={machine}
									nameOf={nameOf}
									job={jobOf(tools.data, tool.id)}
									onOpen={onOpen}
								/>
							))}
						</tbody>
					</table>
				</div>
			)}
		</div>
	);
}

function Runtime({ runtime }: { runtime: RuntimeDto }) {
	return (
		<span className="flex items-center gap-1" title={runtime.path ?? undefined}>
			<span className={runtime.version ? "text-foreground" : "text-muted-foreground"}>
				{runtime.name}
			</span>
			<span className="text-muted-foreground font-mono text-xs">{runtime.version ?? "—"}</span>
		</span>
	);
}

function Row({
	tool,
	machine,
	nameOf,
	job,
	onOpen,
}: {
	tool: ToolStatusDto;
	machine: Machine;
	nameOf: (id: string) => string;
	job: ReturnType<typeof jobOf>;
	onOpen?: (tool: ToolStatusDto) => void;
}) {
	const { t } = useTranslation(["tools", "common"]);
	const install = useInstallTool(machine.id);
	const update = useUpdateTool(machine.id);
	const remove = useRemoveTool(machine.id);
	const actions = actionsOf(tool);
	const busy =
		isRunning(job) || install.isPending || update.isPending || remove.isPending || !machine.online;
	const variables = { id: machine.id, tool: tool.id };
	const languages = tool.languages.map(nameOf);
	const showJob = job && job.state !== "done";
	const version =
		tool.updateAvailable && tool.foundVersion && tool.version
			? `${tool.foundVersion} → ${tool.version}`
			: (tool.foundVersion ?? tool.version);

	return (
		<Fragment>
			<tr className="hover:bg-accent/40 align-top transition-colors duration-150">
				<td className="py-2 pr-3" title={formatList(languages, "unit")}>
					{languages[0] ?? "—"}
					{languages.length > 1 && (
						<span className="text-muted-foreground text-xs"> +{languages.length - 1}</span>
					)}
				</td>
				<td className="py-2 pr-3">
					<span className="flex flex-wrap items-center gap-1.5">
						{onOpen ? (
							<button
								type="button"
								onClick={() => onOpen(tool)}
								aria-label={t("machineTools.openSettings", { tool: tool.name })}
								className="text-heading font-medium hover:underline"
							>
								{tool.name}
							</button>
						) : (
							<span className="text-heading font-medium">{tool.name}</span>
						)}
						{tool.default && <span className="pill pill-muted">{t("machineTools.default")}</span>}
						{tool.origin === "machine" && (
							<span className="pill pill-muted">{t("machineTools.thisMachine")}</span>
						)}
						{tool.origin !== "ahab" && tool.origin !== "machine" && (
							<span className="pill pill-muted" title={t("machineTools.fromPlugin")}>
								{tool.origin}
							</span>
						)}
					</span>
				</td>
				<td className="text-muted-foreground py-2 pr-3">{kindLabel(tool.kind)}</td>
				<td className="py-2 pr-3">
					<span className="flex items-center gap-1.5">
						<span className={cn("size-2 shrink-0 rounded-full", DOT[stateTone(tool.state)])} />
						<span>{stateWord(tool)}</span>
					</span>
					{tool.state === "missing" && tool.hint && (
						<code className="text-muted-foreground mt-0.5 block font-mono text-xs">
							{tool.hint}
						</code>
					)}
				</td>
				<td className="text-muted-foreground py-2 pr-3 font-mono text-xs">{version ?? "—"}</td>
				<td className="py-1.5 text-right whitespace-nowrap">
					{actions.install && (
						<Button
							size="xs"
							variant="outline"
							disabled={busy}
							onClick={() => install.mutate(variables, { onError: toastError })}
						>
							{t("machineTools.install")}
						</Button>
					)}
					{actions.update && (
						<Button
							size="xs"
							variant="outline"
							disabled={busy}
							onClick={() => update.mutate(variables, { onError: toastError })}
						>
							{t("machineTools.update")}
						</Button>
					)}
					{actions.remove && (
						<Button
							size="xs"
							variant="ghost"
							disabled={busy}
							onClick={() => remove.mutate(variables, { onError: toastError })}
						>
							{t("common:actions.remove")}
						</Button>
					)}
				</td>
			</tr>
			{showJob && (
				<tr>
					<td colSpan={6} className="pb-2">
						<ToolJob job={job} />
					</td>
				</tr>
			)}
		</Fragment>
	);
}
