import { cn } from "cn";
import { Cloud, GitBranch, GitFork, Tag } from "lucide-react";
import { useMemo, useState } from "react";

import { useBranches, useCommitDetail, useGraphLog } from "@/modules/projects/git/api";
import { LETTER_TONE, Chevron, Counts } from "@/modules/projects/git/components/changes";
import type { GitOpeners } from "@/modules/projects/git/components/git-view";
import {
	type Badge,
	badges,
	graph,
	type GraphRow,
	statusLetter,
} from "@/modules/projects/git/model";
import type { GitCommit } from "@/shared/api/generated/ahab.schemas";
import { ago } from "@/shared/format";
import { formatDate, useTranslation } from "@/shared/i18n";
import {
	gitGraphAll,
	gitGraphHeight,
	rememberGitGraphAll,
	rememberGitGraphHeight,
} from "@/shared/preferences/store";
import { FileIcon } from "@/shared/ui/file-icon";
import { IconButton } from "@/shared/ui/icon-button";

/** As `toLocaleString()` writes a commit's time: its date and time. */
const COMMIT_TIME: Intl.DateTimeFormatOptions = {
	year: "numeric",
	month: "numeric",
	day: "numeric",
	hour: "numeric",
	minute: "numeric",
	second: "numeric",
};

/** Lanes' colours, in turn; lane 0 (the first line) is the app's own. */
const LANE_COLORS = [
	"var(--primary)",
	"#e5a03a",
	"#3fb27f",
	"#d1608f",
	"#4a9fd8",
	"#a77ee0",
	"#d9734a",
	"#7d9a3a",
];
const LANE = 12;
const ROW = 26;
/** Lanes beyond these are cut off; their commits still show. */
const MAX_LANES = 10;
/** The graph pane's least height, and the room it leaves above itself. */
const MIN_HEIGHT = 120;
const MIN_ABOVE = 96;

function color(lane: number): string {
	return LANE_COLORS[lane % LANE_COLORS.length] ?? "var(--primary)";
}

function x(lane: number): number {
	return LANE / 2 + 2 + lane * LANE;
}

/**
 * The repository's commits as a graph, as VS Code's: each line a lane, from
 * the commits' parents, with branches and tags on the commits they point
 * at. A commit opens to its files, each a diff against its parent.
 */
export function GraphView({
	projectId,
	repository,
	name,
	onOpenDiff,
}: {
	projectId: string;
	repository?: string;
	/** The repository's name, when the project has several. */
	name?: string;
	onOpenDiff: GitOpeners["onOpenDiff"];
}) {
	const { t } = useTranslation("projects");
	const [open, setOpen] = useState(true);
	const [all, setAll] = useState(gitGraphAll);
	const [height, setHeight] = useState(gitGraphHeight);
	const [shown, setShown] = useState<string>();
	const log = useGraphLog(projectId, repository, all, open);
	const branches = useBranches(projectId, repository, open);
	const commits = useMemo(() => log.data?.pages.flatMap((page) => page.commits) ?? [], [log.data]);
	const lanes = useMemo(() => graph(commits), [commits]);
	const remotes = useMemo(
		() => [
			...new Set(
				(branches.data?.remote ?? []).map((branch) =>
					branch.name.slice(0, branch.name.indexOf("/")),
				),
			),
		],
		[branches.data],
	);
	const width = Math.min(lanes.width, MAX_LANES) * LANE + 4;
	const { hasNextPage, isFetchingNextPage, fetchNextPage } = log;
	// Older commits load as the list nears its end.
	const onScroll = (event: React.UIEvent<HTMLElement>) => {
		const list = event.currentTarget;
		if (
			hasNextPage &&
			!isFetchingNextPage &&
			list.scrollTop + list.clientHeight > list.scrollHeight - 400
		)
			void fetchNextPage();
	};
	// Dragging the top edge sets the pane's height (it still fills free room).
	const resize = (event: React.PointerEvent<HTMLElement>) => {
		event.preventDefault();
		const start = event.clientY;
		const from = event.currentTarget.parentElement?.getBoundingClientRect().height ?? height;
		let next = from;
		const move = (moved: PointerEvent) => {
			next = Math.max(MIN_HEIGHT, Math.round(from - (moved.clientY - start)));
			setHeight(next);
		};
		const up = () => {
			window.removeEventListener("pointermove", move);
			window.removeEventListener("pointerup", up);
			rememberGitGraphHeight(next);
		};
		window.addEventListener("pointermove", move);
		window.addEventListener("pointerup", up);
	};

	return (
		<section
			aria-label={t("graphView.title")}
			className={cn(
				"border-border relative flex flex-col border-t",
				open ? "min-h-0" : "flex-none",
			)}
			style={
				open ? { flex: `1 0 ${height}px`, maxHeight: `calc(100% - ${MIN_ABOVE}px)` } : undefined
			}
		>
			{open ? (
				<div
					role="separator"
					aria-orientation="horizontal"
					aria-label={t("graphView.resize")}
					onPointerDown={resize}
					className="hover:bg-primary/40 active:bg-primary/60 absolute inset-x-0 -top-0.5 z-10 h-1 cursor-row-resize"
				/>
			) : null}
			<div className="group/head flex h-7 shrink-0 items-center pr-1.5 pl-1">
				<Chevron
					open={open}
					onClick={() => setOpen((value) => !value)}
					label={t("graphView.title")}
				/>
				<button
					type="button"
					onClick={() => setOpen((value) => !value)}
					className="text-muted-foreground hover:text-foreground flex min-w-0 flex-1 items-center gap-1.5 pl-1 text-left text-[0.6875rem] font-semibold tracking-wider uppercase"
				>
					{t("graphView.title")}
					{name ? (
						<span className="truncate font-normal tracking-normal normal-case">{name}</span>
					) : null}
				</button>
				{open ? (
					<span className="flex gap-0.5 md:opacity-0 md:group-focus-within/head:opacity-100 md:group-hover/head:opacity-100">
						<IconButton
							label={all ? t("graphView.showCurrent") : t("graphView.showAll")}
							onClick={() => {
								rememberGitGraphAll(!all);
								setAll(!all);
							}}
						>
							{all ? <GitFork /> : <GitBranch />}
						</IconButton>
					</span>
				) : null}
			</div>
			{open ? (
				<ul
					aria-label={t("graphView.commits")}
					onScroll={onScroll}
					className="min-h-0 flex-1 overflow-auto pb-2"
				>
					{commits.map((commit, index) => {
						const row = lanes.rows[index];
						return row ? (
							<CommitRow
								key={commit.hash}
								commit={commit}
								row={row}
								width={width}
								remotes={remotes}
								open={shown === commit.hash}
								onToggle={() => setShown(shown === commit.hash ? undefined : commit.hash)}
								projectId={projectId}
								repository={repository}
								onOpenDiff={onOpenDiff}
							/>
						) : null;
					})}
					{hasNextPage ? (
						<li>
							<button
								type="button"
								disabled={isFetchingNextPage}
								onClick={() => void fetchNextPage()}
								className="text-muted-foreground hover:text-foreground py-1 text-xs"
								style={{ paddingLeft: width + 8 }}
							>
								{isFetchingNextPage ? t("graphView.loadingOlder") : t("graphView.showOlder")}
							</button>
						</li>
					) : null}
					{log.isPending ? (
						<li className="text-muted-foreground px-3 py-1 text-sm">
							{t("state.loading", { ns: "common" })}
						</li>
					) : commits.length === 0 ? (
						<li className="text-muted-foreground px-3 py-1 text-sm">{t("graphView.empty")}</li>
					) : null}
				</ul>
			) : null}
		</section>
	);
}

function CommitRow({
	commit,
	row,
	width,
	remotes,
	open,
	onToggle,
	projectId,
	repository,
	onOpenDiff,
}: {
	commit: GitCommit;
	row: GraphRow;
	width: number;
	remotes: string[];
	open: boolean;
	onToggle: () => void;
	projectId: string;
	repository?: string;
	onOpenDiff: GitOpeners["onOpenDiff"];
}) {
	const { t } = useTranslation("projects");
	const when = formatDate(commit.time * 1000, COMMIT_TIME);
	const refs = badges(commit.refs ?? [], remotes);
	const head = (commit.refs ?? []).includes("HEAD");
	const detail = useCommitDetail(projectId, repository, open ? commit.hash : undefined);
	return (
		// Rows off screen are skipped by the browser: thousands stay cheap.
		<li className="[contain-intrinsic-size:auto_26px] [content-visibility:auto]">
			<button
				type="button"
				onClick={onToggle}
				aria-expanded={open}
				title={`${commit.short} · ${commit.author} · ${when}`}
				className={cn(
					"hover:bg-accent/60 flex w-full items-center gap-1.5 pr-2 text-left text-[0.8125rem]",
					open && "bg-accent/40",
				)}
				style={{ height: ROW }}
			>
				<Lanes row={row} width={width} head={head} />
				<span className={cn("min-w-[45%] flex-1 truncate", head && "text-heading font-medium")}>
					{commit.subject}
				</span>
				<Badges refs={refs} />
				<span className="text-muted-foreground w-7 shrink-0 text-right text-[0.6875rem] tabular-nums">
					{ago(commit.time * 1000)}
				</span>
			</button>
			{open ? (
				<div className="relative pr-2 pb-1.5" style={{ paddingLeft: width + 6 }}>
					<Below row={row} width={width} />
					<p className="text-muted-foreground pt-0.5 text-[0.6875rem]">
						<span className="font-mono">{commit.short}</span> · {commit.author} · {when}
					</p>
					{refs.length > 1 ? (
						<p className="mt-1 flex flex-wrap gap-1">
							{refs.map((badge) => (
								<BadgeChip key={`${badge.kind}:${badge.name}`} badge={badge} />
							))}
						</p>
					) : null}
					{detail.data?.body ? (
						<p className="text-muted-foreground my-1 text-xs whitespace-pre-wrap">
							{detail.data.body}
						</p>
					) : null}
					{detail.isPending ? (
						<p className="text-muted-foreground text-xs">{t("state.loading", { ns: "common" })}</p>
					) : (
						<ul className="mt-0.5">
							{(detail.data?.files ?? []).map((file) => {
								const letter = statusLetter(file.status);
								const slash = file.path.lastIndexOf("/");
								return (
									<li key={file.path}>
										<button
											type="button"
											onClick={() => onOpenDiff(file, "commit", commit.hash)}
											title={file.path}
											className="hover:bg-accent/60 flex h-6 w-full items-center gap-1.5 rounded pr-1 text-left text-xs"
										>
											<FileIcon path={file.path} className="size-3.5" />
											<span className="min-w-0 flex-1 truncate">
												{file.path.slice(slash + 1)}
												<span className="text-muted-foreground ml-1.5 text-[0.6875rem]">
													{file.path.slice(0, Math.max(0, slash))}
												</span>
											</span>
											<Counts change={file} />
											<span
												className={cn(
													"w-3 shrink-0 text-center font-mono font-semibold",
													LETTER_TONE[letter],
												)}
											>
												{letter}
											</span>
										</button>
									</li>
								);
							})}
						</ul>
					)}
				</div>
			) : null}
		</li>
	);
}

/** One row of lanes: lines through, lines merging in, the commit, lines out. */
function Lanes({ row, width, head }: { row: GraphRow; width: number; head: boolean }) {
	const mid = ROW / 2;
	const at = x(row.lane);
	return (
		<svg aria-hidden width={width} height={ROW} className="shrink-0 overflow-hidden">
			<g fill="none" strokeWidth={1.75} strokeLinecap="round">
				{row.through.map((lane) => (
					<path key={`t${lane}`} d={`M${x(lane)} 0V${ROW}`} stroke={color(lane)} />
				))}
				{row.into.map((lane) => (
					<path
						key={`i${lane}`}
						d={`M${x(lane)} 0C${x(lane)} ${mid} ${at} ${mid / 2} ${at} ${mid}`}
						stroke={color(lane)}
					/>
				))}
				{row.joins.map((lane) => (
					<path
						key={`j${lane}`}
						d={`M${x(lane)} 0C${x(lane)} ${mid} ${at} ${mid} ${at} ${ROW}`}
						stroke={color(lane)}
					/>
				))}
				{row.child ? <path d={`M${at} 0V${mid}`} stroke={color(row.lane)} /> : null}
				{row.out.map((lane) =>
					lane === row.lane ? (
						<path key={`o${lane}`} d={`M${at} ${mid}V${ROW}`} stroke={color(lane)} />
					) : (
						<path
							key={`o${lane}`}
							d={`M${at} ${mid}C${at} ${mid + mid / 2} ${x(lane)} ${mid} ${x(lane)} ${ROW}`}
							stroke={color(lane)}
						/>
					),
				)}
			</g>
			{head ? (
				<>
					<circle
						cx={at}
						cy={mid}
						r={5}
						fill="var(--background)"
						stroke={color(row.lane)}
						strokeWidth={2}
					/>
					<circle cx={at} cy={mid} r={2} fill={color(row.lane)} />
				</>
			) : (
				<circle
					cx={at}
					cy={mid}
					r={3.5}
					fill={color(row.lane)}
					stroke="var(--background)"
					strokeWidth={1.5}
				/>
			)}
		</svg>
	);
}

/** The lanes carrying on under an opened commit, down to the next one. */
function Below({ row, width }: { row: GraphRow; width: number }) {
	return (
		<svg
			aria-hidden
			width={width}
			height="100%"
			className="absolute inset-y-0 left-0"
			preserveAspectRatio="none"
		>
			{row.below.map((lane) => (
				<line
					key={lane}
					x1={x(lane)}
					x2={x(lane)}
					y1="0"
					y2="100%"
					stroke={color(lane)}
					strokeWidth={1.75}
				/>
			))}
		</svg>
	);
}

function Badges({ refs }: { refs: Badge[] }) {
	if (refs.length === 0) return null;
	const shown = refs.slice(0, 1);
	const more = refs.length - shown.length;
	return (
		<span className="flex max-w-[45%] min-w-0 shrink items-center justify-end gap-1">
			{shown.map((badge) => (
				<BadgeChip key={`${badge.kind}:${badge.name}`} badge={badge} />
			))}
			{more > 0 ? (
				<span
					className="text-muted-foreground shrink-0 text-[0.625rem]"
					title={refs
						.slice(1)
						.map((badge) => badge.name)
						.join(", ")}
				>
					+{more}
				</span>
			) : null}
		</span>
	);
}

function BadgeChip({ badge }: { badge: Badge }) {
	const { t } = useTranslation("projects");
	const Icon = badge.kind === "tag" ? Tag : badge.kind === "remote" ? Cloud : GitBranch;
	return (
		<span
			title={badge.kind === "head" ? t("graphView.current", { name: badge.name }) : badge.name}
			className={cn(
				"flex h-[1.125rem] min-w-0 items-center gap-0.5 rounded-full border px-1.5 text-[0.6562rem] leading-none [&_svg]:size-2.5 [&_svg]:shrink-0",
				badge.kind === "head"
					? "border-primary/50 bg-primary/12 text-primary font-medium"
					: badge.kind === "tag"
						? "border-warning/40 text-warning"
						: badge.kind === "remote"
							? "border-border text-muted-foreground"
							: "border-border text-foreground",
			)}
		>
			<Icon />
			<span className="truncate">{badge.name}</span>
		</span>
	);
}
