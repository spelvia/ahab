import { cn } from "cn";
import { ChevronsUpDown, Columns2, FileText, Minus, Plus, Rows2, X } from "lucide-react";
import { createContext, Fragment, useContext, useEffect, useMemo, useState } from "react";

import { useDiff, useGitActions } from "@/modules/projects/git/api";
import {
	allBut,
	applyLines,
	type Cell,
	fold,
	type Hunk,
	hunks,
	hunkText,
	type Row,
	sideBySide,
	type Taken,
} from "@/modules/projects/git/model";
import type { GitDiffGroup } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { AddToChat } from "@/shared/chat-context/add-to-chat";
import { useChatContext } from "@/shared/chat-context/context";
import { type ContextReference, referenceId } from "@/shared/chat-context/model";
import type { HighlightToken } from "@/shared/editor/model";
import { useTranslation } from "@/shared/i18n";
import { HighlightedLine, useHighlightedLines } from "@/shared/syntax/code";
import { IconButton } from "@/shared/ui/icon-button";
import { Note } from "@/shared/ui/note";
import { toast, toastError } from "@/shared/ui/toast";

const GROUPS = {
	changes: "diffView.group.changes",
	staged: "diffView.group.staged",
	untracked: "diffView.group.untracked",
	conflicts: "diffView.group.conflicts",
	commit: "diffView.group.commit",
} as const satisfies Record<GitDiffGroup, string>;

/** Whether the screen is too narrow for two columns. */
function narrow(): boolean {
	return typeof window !== "undefined" && window.matchMedia?.("(max-width: 767px)").matches;
}

/**
 * A file's changes, side by side: the old version on the left, the new on
 * the right, changed words marked, long unchanged stretches folded. One
 * column on narrow screens, or when chosen.
 */
/** The context a hunk gives a chat: its diff, at the file's new lines. */
function hunkContext(rows: Row[], hunk: Hunk, projectId: string, path: string): ContextReference[] {
	const { text, startLine, endLine } = hunkText(rows, hunk);
	return [{ id: referenceId(), kind: "diffHunk", projectId, path, startLine, endLine, text }];
}

const EMPTY_ROWS: Row[] = [];

/** Large rewrites belong off the thread that paints and handles input. */
function useDiffRows(before: string, after: string) {
	const offload = typeof Worker !== "undefined" && before.length + after.length > 24_000;
	const [computed, setComputed] = useState<{ before: string; after: string; rows: Row[] }>();
	useEffect(() => {
		if (!offload) return;
		const fallback = () => setComputed({ before, after, rows: sideBySide(before, after) });
		let worker: Worker;
		try {
			worker = new Worker(new URL("./diff-worker.ts", import.meta.url), { type: "module" });
		} catch {
			const timer = setTimeout(fallback, 0);
			return () => clearTimeout(timer);
		}
		worker.addEventListener(
			"message",
			(event: MessageEvent<Row[]>) => {
				setComputed({ before, after, rows: event.data });
				worker.terminate();
			},
			{ once: true },
		);
		worker.addEventListener(
			"error",
			() => {
				worker.terminate();
				fallback();
			},
			{ once: true },
		);
		worker.postMessage({ before, after }, []);
		return () => worker.terminate();
	}, [before, after, offload]);
	return useMemo(() => {
		if (!before && !after) return EMPTY_ROWS;
		if (!offload) return sideBySide(before, after);
		if (computed?.before === before && computed.after === after) return computed.rows;
	}, [before, after, offload, computed]);
}

export function DiffView({
	projectId,
	repository,
	path,
	oldPath,
	group,
	commit,
	onOpenFile,
	filePath,
}: {
	projectId: string;
	/** A repository inside the project's folder; absent for the one it is in. */
	repository?: string;
	/** Relative to the repository. */
	path: string;
	oldPath?: string;
	group: GitDiffGroup;
	commit?: string;
	/** Opens the file itself (when it exists in the working tree). */
	onOpenFile?: () => void;
	/** The file as a path of the project, for context sent to a chat. */
	filePath?: string;
}) {
	const { t } = useTranslation("projects");
	const chat = useChatContext();
	const diff = useDiff(projectId, { path, oldPath, group, commit, repository });
	const [unified, setUnified] = useState(narrow);
	const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
	const plainDiff = diff.data && !diff.data.binary && !diff.data.tooLarge ? diff.data : undefined;
	const calculated = useDiffRows(plainDiff?.old ?? "", plainDiff?.new ?? "");
	const rows = calculated ?? EMPTY_ROWS;
	const shown = useMemo(() => fold(rows, open), [rows, open]);
	// Both sides highlighted as the file's language.
	const oldTokens = useHighlightedLines(diff.data?.old ?? undefined, { path: oldPath ?? path });
	const newTokens = useHighlightedLines(diff.data?.new ?? undefined, { path });
	const tokens = useMemo(() => ({ old: oldTokens, new: newTokens }), [oldTokens, newTokens]);
	const changed = rows.some((row) => row.kind !== "same");
	const width = String(rows.length + 1).length;

	// Staging part of the file: a hunk at a time, or chosen lines.
	const actions = useGitActions(projectId);
	const unstaging = group === "staged";
	const partial = Boolean(diff.data?.partial) && !commit;
	// Chosen rows belong to the rows they were chosen in: when the diff
	// changes underneath, they are dropped rather than point elsewhere.
	const [choice, setChoice] = useState<{ rows: Row[]; set: ReadonlySet<number> }>({
		rows,
		set: new Set(),
	});
	const chosen: ReadonlySet<number> = choice.rows === rows ? choice.set : new Set();
	const setChosen = (
		next: ReadonlySet<number> | ((set: ReadonlySet<number>) => ReadonlySet<number>),
	) =>
		setChoice((current) => {
			const base = current.rows === rows ? current.set : new Set<number>();
			return { rows, set: typeof next === "function" ? next(base) : next };
		});
	const [last, setLast] = useState<number>();
	const position = useMemo(() => new Map(rows.map((row, index) => [row, index])), [rows]);
	const firstOf = useMemo(() => new Map(hunks(rows).map((hunk) => [hunk.at, hunk])), [rows]);
	const linesOf = (indices: Iterable<number>): Taken => {
		const removed = new Set<number>();
		const added = new Set<number>();
		for (const index of indices) {
			const row = rows[index];
			if (!row || row.kind === "same" || row.kind === "fold") continue;
			if ("left" in row) removed.add(row.left.line);
			if ("right" in row) added.add(row.right.line);
		}
		return { removed, added };
	};
	/** Stages the lines taken; `lines` counts them, absent for a hunk. */
	const stage = (take: Taken, lines?: number) => {
		if (!diff.data) return;
		const before = diff.data.old ?? "";
		const after = diff.data.new ?? "";
		const content = applyLines(before, after, unstaging ? allBut(rows, take) : take);
		actions.stagePart.mutate(
			{
				id: projectId,
				params: repository ? { repository } : undefined,
				data: { path, content, index: diff.data.index ?? null },
			},
			{
				onSuccess: () => {
					setChosen(new Set());
					toast(
						lines === undefined
							? unstaging
								? t("diffView.hunkUnstaged")
								: t("diffView.hunkStaged")
							: unstaging
								? t("diffView.linesUnstaged", { count: lines })
								: t("diffView.linesStaged", { count: lines }),
					);
				},
				onError: toastError,
			},
		);
	};
	const choose = (index: number, range: boolean) => {
		setChosen((set) => {
			const next = new Set(set);
			if (range && last !== undefined) {
				const [from, to] = last < index ? [last, index] : [index, last];
				for (let i = from; i <= to; i++) if (rows[i]?.kind !== "same") next.add(i);
			} else if (next.has(index)) next.delete(index);
			else next.add(index);
			return next;
		});
		setLast(index);
	};
	const pick = partial ? choose : undefined;

	return (
		<div className="bg-background flex h-full min-h-0 flex-col">
			<div className="text-muted-foreground flex h-8 shrink-0 items-center gap-2 px-4 text-xs">
				<span className="text-foreground truncate font-mono">
					{oldPath && oldPath !== path ? `${oldPath} → ${path}` : path}
				</span>
				<span className="bg-muted rounded px-1.5 py-0.5 text-[0.6875rem]">
					{t(GROUPS[group])}
					{commit ? ` ${commit.slice(0, 7)}` : ""}
				</span>
				<span className="ml-auto flex items-center gap-0.5">
					{partial && chosen.size > 0 ? (
						<>
							<button
								type="button"
								disabled={actions.stagePart.isPending}
								onClick={() => stage(linesOf(chosen), chosen.size)}
								className="bg-primary text-primary-foreground hover:bg-primary/90 flex h-6 items-center gap-1 rounded px-2 text-xs font-medium disabled:opacity-60"
							>
								{unstaging ? <Minus className="size-3" /> : <Plus className="size-3" />}
								{unstaging
									? t("diffView.unstageLines", { count: chosen.size })
									: t("diffView.stageLines", { count: chosen.size })}
							</button>
							<IconButton label={t("diffView.clearChosen")} onClick={() => setChosen(new Set())}>
								<X />
							</IconButton>
						</>
					) : null}
					{onOpenFile ? (
						<IconButton label={t("diffView.openFile")} onClick={onOpenFile}>
							<FileText />
						</IconButton>
					) : null}
					<IconButton
						label={unified ? t("diffView.sideBySide") : t("diffView.oneColumn")}
						onClick={() => setUnified((value) => !value)}
						className="max-md:hidden"
					>
						{unified ? <Columns2 /> : <Rows2 />}
					</IconButton>
				</span>
			</div>
			<div className="min-h-0 flex-1 overflow-auto">
				{diff.isPending ? (
					<Note>{t("state.loading", { ns: "common" })}</Note>
				) : diff.isError ? (
					<Note>{explain(diff.error, t("diffView.cannotShow"))}</Note>
				) : diff.data.binary ? (
					<Note>{t("diffView.binary")}</Note>
				) : diff.data.tooLarge ? (
					<Note>{t("diffView.tooLarge")}</Note>
				) : !calculated ? (
					<Note>{t("state.loading", { ns: "common" })}</Note>
				) : !changed ? (
					<Note>{t("diffView.noChanges")}</Note>
				) : (
					<DiffTokens.Provider value={tokens}>
						<div
							className="min-w-max py-1 font-mono text-[0.7812rem] leading-5"
							style={{ "--gutter": `${width + 2}ch` } as React.CSSProperties}
						>
							{shown.map((row) => {
								if (row.kind === "fold")
									return (
										<button
											key={`fold-${row.at}`}
											type="button"
											onClick={() => setOpen((set) => new Set(set).add(row.at))}
											className="text-muted-foreground bg-muted/40 hover:bg-muted flex w-full items-center gap-2 px-4 py-0.5 text-left text-xs"
										>
											<ChevronsUpDown className="size-3.5" />
											{t("diffView.unchangedLines", { count: row.hidden })}
										</button>
									);
								const index = position.get(row) ?? -1;
								const hunk = partial || chat ? firstOf.get(index) : undefined;
								const onPick =
									pick && row.kind !== "same" ? (range: boolean) => pick(index, range) : undefined;
								const Shown = unified ? UnifiedRow : SplitRow;
								return (
									<Fragment key={key(row)}>
										{hunk ? (
											<HunkBar
												unstaging={unstaging}
												disabled={actions.stagePart.isPending}
												onStage={
													partial
														? () =>
																stage({
																	removed: new Set(hunk.removed),
																	added: new Set(hunk.added),
																})
														: undefined
												}
												context={
													chat
														? () => hunkContext(rows, hunk, projectId, filePath ?? path)
														: undefined
												}
											/>
										) : null}
										<Shown row={row} chosen={chosen.has(index)} onPick={onPick} />
									</Fragment>
								);
							})}
						</div>
					</DiffTokens.Provider>
				)}
			</div>
		</div>
	);
}

function key(row: Exclude<Row, { kind: "fold" }>): string {
	const left = "left" in row ? row.left.line : "";
	const right = "right" in row ? row.right.line : "";
	return `${left}:${right}`;
}

const TONE = {
	remove: "bg-destructive/15",
	add: "bg-success/18",
	removeWord: "bg-destructive/35 rounded-[2px]",
	addWord: "bg-success/40 rounded-[2px]",
};

/** Each side's tokens by line (from 0), once highlighted. */
const DiffTokens = createContext<{
	old: HighlightToken[][] | null;
	new: HighlightToken[][] | null;
}>({
	old: null,
	new: null,
});

/** Where a changed piece of a line is, for marking it. */
function changedRanges(pieces: NonNullable<Cell["pieces"]>): { from: number; to: number }[] {
	const ranges: { from: number; to: number }[] = [];
	let at = 0;
	for (const piece of pieces) {
		if (piece.changed) ranges.push({ from: at, to: at + piece.text.length });
		at += piece.text.length;
	}
	return ranges;
}

/** Picks a changed row (shift: the rows since the last one picked). */
type OnPick = (range: boolean) => void;

function Line({
	cell,
	side,
	tone,
	words,
	chosen,
	onPick,
}: {
	cell?: Cell;
	/** Which version of the file the cell's line is from. */
	side: "old" | "new";
	tone?: "remove" | "add";
	words?: "removeWord" | "addWord";
	chosen?: boolean;
	onPick?: OnPick;
}) {
	const { t } = useTranslation("projects");
	const tokens = useContext(DiffTokens)[side];
	const lineTokens = cell ? tokens?.[cell.line - 1] : undefined;
	return (
		<div
			className={cn(
				"flex min-w-0 flex-1",
				tone && TONE[tone],
				!cell && "bg-muted/30",
				chosen && "shadow-[inset_3px_0_0_var(--primary)]",
			)}
		>
			{onPick && cell ? (
				<button
					type="button"
					aria-label={
						chosen
							? t("diffView.unchooseLine", { line: cell.line })
							: t("diffView.chooseLine", { line: cell.line })
					}
					aria-pressed={chosen}
					title={t("diffView.chooseLineTitle")}
					onClick={(event) => onPick(event.shiftKey)}
					className={cn(
						"hover:text-foreground shrink-0 cursor-pointer pr-2 text-right select-none",
						chosen ? "text-primary font-semibold" : "text-muted-foreground/70",
					)}
					style={{ width: "var(--gutter)" }}
				>
					{cell.line}
				</button>
			) : (
				<span
					aria-hidden
					className="text-muted-foreground/70 shrink-0 pr-2 text-right select-none"
					style={{ width: "var(--gutter)" }}
				>
					{cell?.line ?? ""}
				</span>
			)}
			<span className="pr-4 whitespace-pre">
				{cell ? (
					<HighlightedLine
						text={cell.text}
						tokens={lineTokens}
						marks={cell.pieces && words ? changedRanges(cell.pieces) : undefined}
						markClass={words ? TONE[words] : undefined}
					/>
				) : null}
				{cell && !cell.text ? " " : null}
			</span>
		</div>
	);
}

interface RowProps {
	row: Exclude<Row, { kind: "fold" }>;
	chosen: boolean;
	onPick?: OnPick;
}

function SplitRow({ row, chosen, onPick }: RowProps) {
	const left = "left" in row ? row.left : undefined;
	const right = "right" in row ? row.right : undefined;
	const changed = row.kind !== "same";
	return (
		<div className="grid grid-cols-2">
			<div className="border-border flex overflow-hidden border-r">
				<Line
					cell={left}
					side="old"
					tone={changed && left ? "remove" : undefined}
					words="removeWord"
					chosen={chosen}
					onPick={onPick}
				/>
			</div>
			<div className="flex overflow-hidden">
				<Line
					cell={right}
					side="new"
					tone={changed && right ? "add" : undefined}
					words="addWord"
					chosen={chosen}
					onPick={left ? undefined : onPick}
				/>
			</div>
		</div>
	);
}

function UnifiedRow({ row, chosen, onPick }: RowProps) {
	if (row.kind === "same") return <Line cell={row.right} side="new" />;
	return (
		<>
			{"left" in row ? (
				<Line
					cell={row.left}
					side="old"
					tone="remove"
					words="removeWord"
					chosen={chosen}
					onPick={onPick}
				/>
			) : null}
			{"right" in row ? (
				<Line
					cell={row.right}
					side="new"
					tone="add"
					words="addWord"
					chosen={chosen}
					onPick={"left" in row ? undefined : onPick}
				/>
			) : null}
		</>
	);
}

/** Above each hunk: stage (or unstage) it at once; add it to a chat. */
function HunkBar({
	unstaging,
	disabled,
	onStage,
	context,
}: {
	unstaging: boolean;
	disabled: boolean;
	onStage?: () => void;
	context?: () => ContextReference[];
}) {
	const { t } = useTranslation("projects");
	return (
		<div className="bg-muted/40 border-border/60 flex h-7 items-center border-y font-sans">
			{/* Pinned to the left edge: the lines may be far wider than the view. */}
			<span className="sticky left-1 flex items-center gap-1.5">
				{onStage ? (
					<button
						type="button"
						disabled={disabled}
						onClick={onStage}
						className="text-muted-foreground hover:text-foreground hover:bg-accent flex h-5 items-center gap-1 rounded px-1.5 text-[0.6875rem] disabled:opacity-60"
					>
						{unstaging ? <Minus className="size-3" /> : <Plus className="size-3" />}
						{unstaging ? t("diffView.unstageHunk") : t("diffView.stageHunk")}
					</button>
				) : null}
				{context ? <AddToChat compact references={context} className="h-5" /> : null}
			</span>
		</div>
	);
}
