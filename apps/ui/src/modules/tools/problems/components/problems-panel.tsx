import { cn } from "cn";
import {
	Check,
	ChevronRight,
	ChevronsDownUp,
	ChevronsUpDown,
	Filter,
	FolderGit2,
	GitBranch,
	Play,
	X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { useRepositories } from "@/modules/projects";
import {
	useCheckRootsLoader,
	useProblems,
	useRepositoryActivity,
	useRunChecks,
} from "@/modules/tools/problems/api";
import { CheckChooser } from "@/modules/tools/problems/components/check-chooser";
import { PinButton } from "@/modules/tools/problems/components/pin-button";
import { ProblemRow } from "@/modules/tools/problems/components/problem-row";
import { SeverityIcon } from "@/modules/tools/problems/components/severity-icon";
import { checkLog, checkRunLines } from "@/modules/tools/problems/hooks";
import {
	ALL_SEVERITIES,
	countOf,
	countText,
	type Checkout,
	type FileProblems,
	filtering,
	groupByRepository,
	type PanelFilters,
	panelMatches,
	parseFilter,
	type SeverityToggles,
	sourcesOf,
} from "@/modules/tools/problems/model";
import { useActivityEvents, useProblemEvents } from "@/modules/tools/problems/stream";
import { useProjectToolSettings, useSaveProjectToolSettings } from "@/modules/tools/setting";
import type { CheckRootDto, CheckRunDto, Project } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { Trans, useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { FileIcon } from "@/shared/ui/file-icon";

type PanelProject = Pick<Project, "id" | "name" | "availability">;

/** Open or closed groups: all open, or all closed, but for those changed since. */
interface Folding {
	collapsed: boolean;
	changed: ReadonlySet<string>;
}

const SEVERITY_ITEMS: { key: keyof SeverityToggles; severity: string }[] = [
	{ key: "errors", severity: "error" },
	{ key: "warnings", severity: "warning" },
	{ key: "infos", severity: "information" },
];

const toolbarButton =
	"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 disabled:opacity-40 [&_svg]:size-3.5";

/**
 * The Problems tab of the bottom panel, as VS Code's: what the
 * language servers, linters, checks and plugins last reported, by file (and
 * by repository and worktree first when the project holds several).
 * Active and pinned repositories show;
 * the rest are one line that opens Check project's chooser. The filter
 * takes words and globs (`**\/*.ts`, `!**\/test/**`); severities, one
 * tool and one repository can be chosen; groups fold. A click opens the
 * file there. Its tools go in the panel's header (`toolbar`). Follows
 * changes as they happen.
 */
export function ProblemsPanel({
	project,
	toolbar,
	onOpenFile,
}: {
	project: PanelProject;
	/** Where the filter and the buttons go: the panel's header. */
	toolbar: HTMLElement | null;
	onOpenFile: (projectId: string, path: string, line?: number) => void;
}) {
	const { t } = useTranslation("tools");
	const [text, setText] = useState("");
	const [severities, setSeverities] = useState<SeverityToggles>(ALL_SEVERITIES);
	const [source, setSource] = useState("");
	const [repository, setRepository] = useState("");
	const [chooser, setChooser] = useState<CheckRootDto[] | null>(null);
	const [folding, setFolding] = useState<Folding>({ collapsed: false, changed: new Set() });
	const filters = useMemo<PanelFilters>(
		() => ({ filter: parseFilter(text), severities, source }),
		[text, severities, source],
	);
	const checks = useRunChecks(project.id);
	const available = project.availability === "available";
	const loadRoots = useCheckRootsLoader();
	const repositories = useRepositories(project.id, available);
	const activity = useRepositoryActivity(project.id, available);
	const checkouts: Checkout[] = useMemo(
		() => (repositories.data && !repositories.data.inside ? repositories.data.repositories : []),
		[repositories.data],
	);
	const repositoryNames = [...new Set(checkouts.map((c) => c.repository ?? c.path))];
	const shown = [project];
	// The tools that reported.
	const [sources, setSources] = useState<Record<string, string[]>>({});
	const allSources = [...new Set(Object.values(sources).flat())].toSorted();
	const filtered = filtering(filters);
	const clear = () => {
		setText("");
		setSeverities(ALL_SEVERITIES);
		setSource("");
		setRepository("");
	};
	const isOpen = (key: string) => folding.collapsed === folding.changed.has(key);
	const toggle = (key: string) =>
		setFolding((now) => {
			const changed = new Set(now.changed);
			if (changed.has(key)) changed.delete(key);
			else changed.add(key);
			return { ...now, changed };
		});
	const runChecks = (only?: string[]) =>
		checks.mutate(
			{ id: project.id, data: only ? { checkouts: only } : {} },
			{
				onSuccess: (data) => checkLog.add(project.id, checkRunLines(data.checks, undefined)),
				onError: (error) =>
					checkLog.add(
						project.id,
						checkRunLines(undefined, explain(error, t("problemsPanel.checksFailed"))),
					),
			},
		);

	/** Where checks would run first: with more than a few roots, which. */
	const checkProject = async () => {
		try {
			const found = await loadRoots(project.id);
			if (found.roots.length > found.askAbove) setChooser(found.roots);
			else runChecks();
		} catch {
			// An older machine cannot say: everywhere, as it always did.
			runChecks();
		}
	};
	const chooseChecks = async () => {
		try {
			setChooser((await loadRoots(project.id)).roots);
		} catch {
			runChecks();
		}
	};

	const tools = (
		<div className="flex min-w-0 flex-1 items-center justify-end gap-0.5">
			<div className="relative min-w-0 flex-1 basis-40 md:max-w-80">
				<input
					aria-label={t("problemsPanel.filter")}
					value={text}
					onChange={(event) => setText(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Escape") setText("");
					}}
					placeholder={t("problemsPanel.filterPlaceholder")}
					title={t("problemsPanel.filterHelp")}
					className="border-input bg-background focus-visible:ring-ring/40 h-6 w-full rounded border px-2 pr-6 text-xs outline-none focus-visible:ring-2"
				/>
				<DropdownMenu>
					<DropdownMenuTrigger
						aria-label={t("problemsPanel.filterMenu")}
						title={t("problemsPanel.filterMenu")}
						className={cn(
							toolbarButton,
							"absolute top-0 right-0 size-6",
							(source ||
								repository ||
								!severities.errors ||
								!severities.warnings ||
								!severities.infos) &&
								"text-primary",
						)}
					>
						<Filter />
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end" className="w-56 text-xs">
						{SEVERITY_ITEMS.map((item) => (
							<DropdownMenuItem
								key={item.key}
								className="text-xs"
								onSelect={(event) => {
									event.preventDefault();
									setSeverities((now) => ({ ...now, [item.key]: !now[item.key] }));
								}}
							>
								<SeverityIcon severity={item.severity} />
								<span className="flex-1">{t(`problemsPanel.severities.${item.key}`)}</span>
								{severities[item.key] ? <Check /> : null}
							</DropdownMenuItem>
						))}
						<DropdownMenuSeparator />
						<DropdownMenuItem className="text-xs" onSelect={() => setSource("")}>
							<span className="flex-1">{t("problemsPanel.everyTool")}</span>
							{source ? null : <Check />}
						</DropdownMenuItem>
						{allSources.map((each) => (
							<DropdownMenuItem key={each} className="text-xs" onSelect={() => setSource(each)}>
								<span className="flex-1">{each}</span>
								{source === each ? <Check /> : null}
							</DropdownMenuItem>
						))}
						{allSources.some((each) => each.includes(".")) ? (
							<PluginSourcesOff
								projectId={project.id}
								sources={allSources.filter((each) => each.includes("."))}
							/>
						) : null}
						{repositoryNames.length > 1 ? (
							<>
								<DropdownMenuSeparator />
								<DropdownMenuItem className="text-xs" onSelect={() => setRepository("")}>
									<span className="flex-1">{t("problemsPanel.everyRepository")}</span>
									{repository ? null : <Check />}
								</DropdownMenuItem>
								{repositoryNames.map((each) => (
									<DropdownMenuItem
										key={each}
										className="text-xs"
										onSelect={() => setRepository(each)}
									>
										<FolderGit2 />
										<span className="flex-1 truncate">{each}</span>
										{repository === each ? <Check /> : null}
									</DropdownMenuItem>
								))}
							</>
						) : null}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
			<button
				type="button"
				className={toolbarButton}
				disabled={checks.isPending || !available}
				onClick={() => void checkProject()}
				aria-label={t("problemsPanel.checkProject")}
				title={
					project.availability === "available"
						? t("problemsPanel.runChecks", { project: project.name })
						: t("problemsPanel.unavailable")
				}
			>
				<Play className={checks.isPending ? "animate-pulse" : undefined} />
			</button>
			<button
				type="button"
				className={toolbarButton}
				onClick={() => setFolding({ collapsed: !folding.collapsed, changed: new Set() })}
				aria-label={
					folding.collapsed ? t("problemsPanel.expandAll") : t("problemsPanel.collapseAll")
				}
				title={folding.collapsed ? t("problemsPanel.expandAll") : t("problemsPanel.collapseAll")}
			>
				{folding.collapsed ? <ChevronsUpDown /> : <ChevronsDownUp />}
			</button>
		</div>
	);

	return (
		<div className="flex h-full flex-col overflow-hidden text-[0.8125rem]">
			{toolbar ? createPortal(tools, toolbar) : <div className="flex px-2 py-1">{tools}</div>}
			<CheckChooser
				open={chooser !== null}
				onOpenChange={(open) => {
					if (!open) setChooser(null);
				}}
				roots={chooser ?? []}
				checkouts={checkouts}
				activity={activity.data?.items ?? []}
				onCheck={(only) => {
					setChooser(null);
					runChecks(only);
				}}
			/>
			{checks.isPending ? (
				<p className="text-muted-foreground border-border border-b px-3 py-1 text-xs">
					{t("problemsPanel.running")}
				</p>
			) : checks.isError ? (
				<p role="alert" className="text-destructive border-border border-b px-3 py-1 text-xs">
					{explain(checks.error, t("problemsPanel.checksFailed"))}
				</p>
			) : checks.data ? (
				<CheckResults checks={checks.data.checks} onDismiss={() => checks.reset()} />
			) : null}
			<div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
				{shown.map((each) => (
					<ProjectProblems
						key={each.id}
						project={each}
						heading={false}
						filters={filters}
						repository={each.id === project.id ? repository : ""}
						filtered={filtered || Boolean(repository)}
						isOpen={isOpen}
						onToggle={toggle}
						onClearFilters={clear}
						onSources={(list) =>
							setSources((now) =>
								now[each.id]?.join() === list.join() ? now : { ...now, [each.id]: list },
							)
						}
						onOpenFile={(path, line) => onOpenFile(each.id, path, line)}
						onChooseChecks={each.id === project.id ? () => void chooseChecks() : undefined}
					/>
				))}
			</div>
		</div>
	);
}

/**
 * One project's problems: by repository and worktree when it holds
 * several, then by file; repositories not active, pinned or checked as one
 * line.
 */
function ProjectProblems({
	project,
	heading,
	filters,
	repository,
	filtered,
	isOpen,
	onToggle,
	onClearFilters,
	onSources,
	onOpenFile,
	onChooseChecks,
}: {
	project: PanelProject;
	heading: boolean;
	filters: PanelFilters;
	/** Only this repository's, when set. */
	repository: string;
	filtered: boolean;
	isOpen: (key: string) => boolean;
	onToggle: (key: string) => void;
	onClearFilters: () => void;
	onSources: (sources: string[]) => void;
	onOpenFile: (path: string, line?: number) => void;
	/** Opens Check project's chooser. */
	onChooseChecks?: () => void;
}) {
	const { t } = useTranslation(["tools", "common"]);
	const available = project.availability === "available";
	const problems = useProblems(project.id);
	useProblemEvents(project.id);
	useActivityEvents(project.id);
	const repositories = useRepositories(project.id, available);
	const activity = useRepositoryActivity(project.id, available);
	const items = useMemo(() => problems.data?.items ?? [], [problems.data]);
	const sources = useMemo(() => sourcesOf(items), [items]);
	const sourcesKey = sources.join("\n");
	const report = useRef(onSources);
	useEffect(() => {
		report.current = onSources;
	});
	useEffect(() => report.current(sourcesKey ? sourcesKey.split("\n") : []), [sourcesKey]);
	const checkouts: Checkout[] = useMemo(
		() => (repositories.data && !repositories.data.inside ? repositories.data.repositories : []),
		[repositories.data],
	);
	const visible = useMemo(
		() =>
			items.filter(
				(problem) =>
					panelMatches(problem, filters) &&
					(!repository ||
						checkouts.some(
							(checkout) =>
								(checkout.repository ?? checkout.path) === repository &&
								(problem.path === checkout.path || problem.path.startsWith(`${checkout.path}/`)),
						)),
			),
		[items, filters, repository, checkouts],
	);
	const grouped = useMemo(
		() => groupByRepository(visible, checkouts, activity.data?.items ?? [], items),
		[visible, checkouts, activity.data, items],
	);
	const byRepository = checkouts.length > 1;
	const key = `w:${project.id}`;
	const open = !heading || isOpen(key);
	const notChecked = byRepository && !repository ? grouped.notChecked.length : 0;
	const files = (list: FileProblems[], prefix: string) =>
		list.map((file) => (
			<FileGroup
				key={file.path}
				file={file}
				prefix={prefix}
				open={isOpen(`${key}:f:${file.path}`)}
				onToggle={() => onToggle(`${key}:f:${file.path}`)}
				onOpenFile={onOpenFile}
			/>
		));
	const notCheckedLine = notChecked ? (
		<li>
			<button
				type="button"
				onClick={onChooseChecks}
				disabled={!onChooseChecks}
				title={t("problemsPanel.notCheckedHelp")}
				className="text-muted-foreground hover:bg-accent/60 hover:text-foreground flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-xs outline-none disabled:hover:bg-transparent"
			>
				<ChevronRight className="size-3.5 shrink-0" />
				<FolderGit2 className="size-3.5 shrink-0" />
				{t("problemsPanel.notChecked", { count: notChecked })}
			</button>
		</li>
	) : null;

	const body = problems.isError ? (
		<p role="alert" className="text-destructive px-2 py-1 text-xs">
			{explain(problems.error, t("problemsPanel.loadFailed"))}
		</p>
	) : !problems.data ? (
		<p className="text-muted-foreground px-2 py-1 text-xs">{t("common:state.loading")}</p>
	) : items.length === 0 && !notChecked ? (
		heading ? null : (
			<p className="text-muted-foreground px-2 py-1">{t("problemsPanel.empty")}</p>
		)
	) : visible.length === 0 && !notChecked ? (
		<p className="text-muted-foreground px-2 py-1">
			{t("problemsPanel.noResults")}{" "}
			{filtered ? (
				<button type="button" className="text-primary hover:underline" onClick={onClearFilters}>
					{t("problemsPanel.clearFilter")}
				</button>
			) : null}
		</p>
	) : !byRepository ? (
		<ul aria-label={t("problemsPanel.problemsIn", { project: project.name })}>
			{files(
				[
					...grouped.loose,
					...grouped.repositories.flatMap((group) => group.checkouts.flatMap((c) => c.files)),
				],
				"",
			)}
		</ul>
	) : (
		<ul aria-label={t("problemsPanel.problemsIn", { project: project.name })}>
			{grouped.repositories.map((group) => {
				const groupKey = `${key}:r:${group.repository}`;
				return (
					<li key={group.repository}>
						<GroupRow
							open={isOpen(groupKey)}
							onToggle={() => onToggle(groupKey)}
							icon={<FolderGit2 className="text-muted-foreground size-3.5 shrink-0" />}
							name={group.repository}
							detail={
								group.single
									? (group.checkouts[0]?.branch ?? undefined)
									: t("problemsPanel.worktrees", { count: group.checkouts.length })
							}
							count={group.checkouts.reduce(
								(sum, c) => sum + c.files.reduce((n, file) => n + file.problems.length, 0),
								0,
							)}
							title={countText(group.counts)}
							actions={
								<PinButton
									projectId={project.id}
									repository={group.repository}
									pinned={group.single ? Boolean(group.checkouts[0]?.pinned) : group.pinned}
								/>
							}
						/>
						{isOpen(groupKey) ? (
							<ul className="pl-3">
								{group.single
									? files(group.checkouts[0]?.files ?? [], group.repository)
									: group.checkouts.map((checkout) => {
											const checkoutKey = `${key}:c:${checkout.path}`;
											return (
												<li key={checkout.path}>
													<GroupRow
														open={isOpen(checkoutKey)}
														onToggle={() => onToggle(checkoutKey)}
														icon={<GitBranch className="text-muted-foreground size-3.5 shrink-0" />}
														name={checkout.label}
														detail={
															checkout.branch && checkout.branch !== checkout.label
																? checkout.branch
																: undefined
														}
														count={checkout.files.reduce((n, file) => n + file.problems.length, 0)}
														title={countText(checkout.counts)}
														actions={
															<PinButton
																projectId={project.id}
																repository={group.repository}
																worktree={checkout.path}
																pinned={checkout.pinned}
															/>
														}
													/>
													{isOpen(checkoutKey) ? (
														<ul className="pl-3">{files(checkout.files, checkout.path)}</ul>
													) : null}
												</li>
											);
										})}
							</ul>
						) : null}
					</li>
				);
			})}
			{grouped.loose.length ? (
				<li>
					<GroupRow
						open={isOpen(`${key}:r:`)}
						onToggle={() => onToggle(`${key}:r:`)}
						icon={<FolderGit2 className="text-muted-foreground size-3.5 shrink-0" />}
						name={project.name}
						detail={t("problemsPanel.outsideRepositories")}
						count={grouped.loose.reduce((n, file) => n + file.problems.length, 0)}
						title={countText(countOf(grouped.loose.flatMap((file) => file.problems)))}
					/>
					{isOpen(`${key}:r:`) ? <ul className="pl-3">{files(grouped.loose, "")}</ul> : null}
				</li>
			) : null}
			{notCheckedLine}
		</ul>
	);

	if (!heading) return body;
	const counts = countOf(visible);
	return (
		<section aria-label={project.name}>
			<GroupRow
				open={open}
				onToggle={() => onToggle(key)}
				name={project.name}
				detail={problems.data && items.length === 0 ? t("problemsPanel.noProblems") : undefined}
				count={visible.length}
				title={countText(counts)}
				strong
			/>
			{open ? <div className="pl-3">{body}</div> : null}
		</section>
	);
}

/** A folding row: a project, a repository or a worktree; its actions beside it. */
function GroupRow({
	open,
	onToggle,
	icon,
	name,
	detail,
	count,
	title,
	strong,
	actions,
}: {
	open: boolean;
	onToggle: () => void;
	icon?: React.ReactNode;
	name: string;
	detail?: string;
	count: number;
	title: string;
	strong?: boolean;
	actions?: React.ReactNode;
}) {
	const row = (
		<button
			type="button"
			aria-expanded={open}
			onClick={onToggle}
			className="hover:bg-accent/60 focus-visible:bg-accent flex w-full items-center gap-1 rounded px-1 py-0.5 text-left outline-none"
		>
			<ChevronRight
				className={cn(
					"text-muted-foreground size-3.5 shrink-0 transition-transform",
					open && "rotate-90",
				)}
			/>
			{icon}
			<span className={cn("truncate", strong ? "font-semibold" : "font-medium")}>{name}</span>
			<span className="text-muted-foreground min-w-0 flex-1 truncate text-[0.6875rem]">
				{detail}
			</span>
			{count ? (
				<span className="bg-muted shrink-0 rounded-full px-1.5 text-[0.625rem]" title={title}>
					{count}
				</span>
			) : null}
		</button>
	);
	if (!actions) return row;
	return (
		<div className="group/row flex items-center gap-0.5">
			<div className="min-w-0 flex-1">{row}</div>
			<div className="opacity-60 group-hover/row:opacity-100 focus-within:opacity-100">
				{actions}
			</div>
		</div>
	);
}

/** A file's problems, under its name and folder. */
function FileGroup({
	file,
	prefix,
	open,
	onToggle,
	onOpenFile,
}: {
	file: FileProblems;
	/** The root it is grouped under, left out of the folder shown. */
	prefix: string;
	open: boolean;
	onToggle: () => void;
	onOpenFile: (path: string, line?: number) => void;
}) {
	const slash = file.path.lastIndexOf("/");
	const folder = file.path.slice(0, Math.max(0, slash));
	const shownFolder =
		prefix && folder.startsWith(prefix) ? folder.slice(prefix.length + 1) : folder;
	return (
		<li>
			<button
				type="button"
				aria-expanded={open}
				onClick={onToggle}
				title={file.path}
				className="hover:bg-accent/60 focus-visible:bg-accent flex w-full items-center gap-1 rounded px-1 py-0.5 text-left outline-none"
			>
				<ChevronRight
					className={cn(
						"text-muted-foreground size-3.5 shrink-0 transition-transform",
						open && "rotate-90",
					)}
				/>
				<FileIcon path={file.path} className="size-3.5" />
				<span className="truncate">{file.path.slice(slash + 1)}</span>
				<span className="text-muted-foreground min-w-0 flex-1 truncate text-[0.6875rem]">
					{shownFolder}
				</span>
				<span
					className="bg-muted shrink-0 rounded-full px-1.5 text-[0.625rem]"
					title={countText(file.counts)}
				>
					{file.problems.length}
				</span>
			</button>
			{open ? (
				<ul>
					{file.problems.map((problem, index) => (
						<ProblemRow
							// The same place may hold two problems from one tool.
							// oxlint-disable-next-line no-array-index-key
							key={`${problem.source}:${problem.line}:${problem.column}:${index}`}
							problem={problem}
							onOpen={() => onOpenFile(problem.path, problem.line)}
						/>
					))}
				</ul>
			) : null}
		</li>
	);
}

/**
 * Turning a plugin's problems off for the project (`plugin.collection`
 * sources), as a tool is turned off; back on in Project settings → LSPs.
 */
function PluginSourcesOff({ projectId, sources }: { projectId: string; sources: string[] }) {
	const { t } = useTranslation("tools");
	const settings = useProjectToolSettings(projectId);
	const saving = useSaveProjectToolSettings(projectId);
	if (!settings.data) return null;
	const current = settings.data;
	return (
		<>
			<DropdownMenuSeparator />
			{sources.map((source) => (
				<DropdownMenuItem
					key={source}
					className="text-xs"
					disabled={saving.isPending}
					onSelect={() =>
						saving.save({
							...current,
							tools: { ...current.tools, [source]: { ...current.tools?.[source], off: true } },
						})
					}
				>
					<span className="flex-1 truncate">{t("problemsPanel.turnOff", { source })}</span>
				</DropdownMenuItem>
			))}
		</>
	);
}

/** What Check project ran: each check, and why one failed. */
function CheckResults({ checks, onDismiss }: { checks: CheckRunDto[]; onDismiss: () => void }) {
	const { t } = useTranslation("tools");
	return (
		<div className="bg-muted/40 border-border relative border-b px-3 py-1.5 text-xs">
			<button
				type="button"
				aria-label={t("problemsPanel.dismiss")}
				onClick={onDismiss}
				className="text-muted-foreground hover:text-foreground absolute top-1.5 right-2"
			>
				<X className="size-3.5" />
			</button>
			{checks.length === 0 ? (
				<p className="text-muted-foreground">{t("problemsPanel.noChecks")}</p>
			) : (
				<ul aria-label={t("problemsPanel.checksRun")} className="flex flex-col gap-0.5 pr-5">
					{checks.map((check) => (
						<li key={`${check.tool}:${check.root}`} className="flex items-start gap-1.5">
							{check.ok ? (
								<Check className="text-muted-foreground mt-0.5 size-3.5 shrink-0" />
							) : (
								<X className="text-destructive mt-0.5 size-3.5 shrink-0" />
							)}
							<span className="min-w-0">
								<Trans
									t={t}
									i18nKey={
										check.root && check.root !== "."
											? "problemsPanel.checkIn"
											: "problemsPanel.check"
									}
									values={{
										name: check.name,
										root: check.root,
										outcome: check.ok
											? countText(check.counts)
											: (check.message ?? t("problemsPanel.checkFailed")),
									}}
									components={{
										name: <span className="font-medium" />,
										root: <span className="text-muted-foreground" />,
									}}
								/>
							</span>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}
