import { cn } from "cn";
import { Bot, Check, ChevronRight, FileText, List, ListTree, Minus, Undo2 } from "lucide-react";
import { useMemo, useState } from "react";

import {
	type Entry,
	type Staged,
	stagedOf,
	statusLetter,
	tree,
	type TreeNode,
} from "@/modules/projects/git/model";
import type { GitChange } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { FileIcon, FolderIcon } from "@/shared/ui/file-icon";
import { IconButton } from "@/shared/ui/icon-button";

export const LETTER_TONE: Record<string, string> = {
	M: "text-warning",
	A: "text-success",
	U: "text-success",
	D: "text-destructive",
	R: "text-primary",
	C: "text-primary",
	T: "text-warning",
	"!": "text-destructive",
};

/** Room for a folder's chevron, so checkboxes line up at every depth. */
const STEP = 12;
const EDGE = 4;

/** What a list of changes can do with its files. */
export interface ChangeActions {
	readOnly: boolean;
	/** Stage these files, or unstage them when they are all staged. */
	onToggle: (paths: string[], staged: Staged) => void;
	onOpen: (entry: Entry<GitChange>) => void;
	/** Opens the file itself; absent when it cannot be (deleted, outside the project). */
	canOpenFile: (change: GitChange) => boolean;
	onOpenFile: (change: GitChange) => void;
	/** Undo the file's unstaged changes (asks first); absent when there are none to undo. */
	canDiscard: (entry: Entry<GitChange>) => boolean;
	onDiscard: (entries: Entry<GitChange>[]) => void;
	/** Files outside the project's folder (in a larger repository) are dimmed. */
	inProject: (path: string) => boolean;
	/** Files the open session's agents edited, marked. */
	edited?: ReadonlySet<string>;
}

/**
 * The one list of changes, as Zed and JetBrains have it: a checked file is
 * staged, a mixed one partly; Commit commits what is checked. A list of
 * paths, or a tree of folders.
 */
export function Changes({
	entries: all,
	edited,
	asTree,
	onTreeChange,
	actions,
}: {
	entries: Entry<GitChange>[];
	/** Files the open session's agents edited, marked; and a filter to them. */
	edited: ReadonlySet<string>;
	asTree: boolean;
	onTreeChange: (tree: boolean) => void;
	actions: ChangeActions;
}) {
	const { t } = useTranslation("projects");
	const [open, setOpen] = useState(true);
	const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
	const [onlyOurs, setOnlyOurs] = useState(false);
	const anyOurs = all.some((entry) => edited.has(entry.path));
	const entries = useMemo(
		() => (onlyOurs && anyOurs ? all.filter((entry) => edited.has(entry.path)) : all),
		[all, edited, onlyOurs, anyOurs],
	);
	const rowActions = useMemo(() => ({ ...actions, edited }), [actions, edited]);
	const nodes = useMemo(() => (asTree ? tree(entries) : []), [asTree, entries]);
	if (entries.length === 0) return null;
	const state = stagedOf(entries);
	const discardable = entries.filter(actions.canDiscard);
	const toggleFolder = (path: string) =>
		setClosed((set) => {
			const next = new Set(set);
			if (next.has(path)) next.delete(path);
			else next.add(path);
			return next;
		});

	return (
		<section aria-label={t("changes.title")}>
			<div className="group/head flex h-7 items-center pr-1.5" style={{ paddingLeft: EDGE }}>
				<Chevron
					open={open}
					onClick={() => setOpen((value) => !value)}
					label={t("changes.title")}
				/>
				<Checkbox
					state={state}
					label={state === "all" ? t("changes.unstageAll") : t("changes.stageAll")}
					disabled={actions.readOnly}
					onClick={() =>
						actions.onToggle(
							entries.map((entry) => entry.path),
							state,
						)
					}
				/>
				<button
					type="button"
					onClick={() => setOpen((value) => !value)}
					className="text-muted-foreground hover:text-foreground flex min-w-0 flex-1 items-center gap-1.5 pl-1.5 text-left text-[0.6875rem] font-semibold tracking-wider uppercase"
				>
					{t("changes.title")}
					<span className="bg-muted rounded-full px-1.5 text-[0.625rem] font-medium tracking-normal">
						{entries.length}
					</span>
				</button>
				<span className="flex gap-0.5 md:opacity-0 md:group-focus-within/head:opacity-100 md:group-hover/head:opacity-100">
					{anyOurs ? (
						<IconButton
							label={onlyOurs ? t("changes.showEvery") : t("changes.onlyThisSession")}
							pressed={onlyOurs}
							onClick={() => setOnlyOurs((value) => !value)}
						>
							<Bot />
						</IconButton>
					) : null}
					<IconButton
						label={asTree ? t("changes.viewAsList") : t("changes.viewAsTree")}
						onClick={() => onTreeChange(!asTree)}
					>
						{asTree ? <List /> : <ListTree />}
					</IconButton>
					{actions.readOnly || discardable.length === 0 ? null : (
						<IconButton
							label={t("changes.discardAll")}
							onClick={() => actions.onDiscard(discardable)}
						>
							<Undo2 />
						</IconButton>
					)}
				</span>
			</div>
			{open ? (
				<ul>
					{asTree
						? nodes.map((node) => (
								<Node
									key={node.kind === "folder" ? `d:${node.path}` : node.entry.path}
									node={node}
									depth={0}
									closed={closed}
									onToggleFolder={toggleFolder}
									actions={rowActions}
								/>
							))
						: entries.map((entry) => (
								<ChangeRow key={entry.path} entry={entry} indent={EDGE + 16} actions={rowActions} />
							))}
				</ul>
			) : null}
		</section>
	);
}

function Node({
	node,
	depth,
	closed,
	onToggleFolder,
	actions,
}: {
	node: TreeNode<Entry<GitChange>>;
	depth: number;
	closed: ReadonlySet<string>;
	onToggleFolder: (path: string) => void;
	actions: ChangeActions;
}) {
	const { t } = useTranslation("projects");
	const indent = EDGE + 16 + depth * STEP;
	if (node.kind === "file")
		return <ChangeRow entry={node.entry} indent={indent} actions={actions} tree />;
	const open = !closed.has(node.path);
	const staged = stagedOf(node.files);
	return (
		<li>
			<div
				className="hover:bg-accent/60 flex h-7 items-center pr-1.5"
				style={{ paddingLeft: indent - 16 }}
			>
				<Chevron open={open} onClick={() => onToggleFolder(node.path)} label={node.name} />
				<Checkbox
					state={staged}
					label={
						staged === "all"
							? t("changes.unstage", { path: node.path })
							: t("changes.stage", { path: node.path })
					}
					disabled={actions.readOnly}
					onClick={() =>
						actions.onToggle(
							node.files.map((entry) => entry.path),
							staged,
						)
					}
				/>
				<button
					type="button"
					aria-expanded={open}
					onClick={() => onToggleFolder(node.path)}
					title={node.path}
					className="flex min-w-0 flex-1 items-center gap-1.5 pl-1.5 text-left text-[0.8125rem]"
				>
					<FolderIcon name={node.name} open={open} className="size-3.5" />
					<span className="truncate">{node.name}</span>
				</button>
			</div>
			{open ? (
				<ul>
					{node.children.map((child) => (
						<Node
							key={child.kind === "folder" ? `d:${child.path}` : child.entry.path}
							node={child}
							depth={depth + 1}
							closed={closed}
							onToggleFolder={onToggleFolder}
							actions={actions}
						/>
					))}
				</ul>
			) : null}
		</li>
	);
}

/** A changed file: checkbox, icon, name and folder, actions, lines, status. */
function ChangeRow({
	entry,
	indent,
	tree: inTree,
	actions,
}: {
	entry: Entry<GitChange>;
	indent: number;
	tree?: boolean;
	actions: ChangeActions;
}) {
	const { t } = useTranslation("projects");
	const { change } = entry;
	const letter = statusLetter(change.status);
	const slash = change.path.lastIndexOf("/");
	const name = change.path.slice(slash + 1);
	const folder = change.path.slice(0, Math.max(0, slash));
	return (
		<li
			className={cn(
				"group/row hover:bg-accent/60 flex h-7 items-center gap-1 pr-1.5",
				!actions.inProject(change.path) && "opacity-60",
			)}
			style={{ paddingLeft: indent }}
		>
			<Checkbox
				state={entry.staged}
				label={
					entry.staged === "all"
						? t("changes.unstage", { path: change.path })
						: t("changes.stage", { path: change.path })
				}
				disabled={actions.readOnly}
				onClick={() => actions.onToggle([entry.path], entry.staged)}
			/>
			<button
				type="button"
				onClick={() => actions.onOpen(entry)}
				title={change.oldPath ? `${change.oldPath} → ${change.path}` : change.path}
				className="flex h-full min-w-0 flex-1 items-center gap-1.5 pl-0.5 text-left text-[0.8125rem]"
			>
				<FileIcon path={change.path} className="size-3.5" />
				{actions.edited?.has(change.path) ? (
					<Bot aria-label={t("changes.editedInSession")} className="text-primary size-3.5 shrink-0">
						<title>{t("changes.editedBySessionAgents")}</title>
					</Bot>
				) : null}
				<span
					className={cn(
						"min-w-0 flex-1 truncate",
						change.status === "deleted" && "line-through decoration-1 opacity-80",
					)}
				>
					{name}
					{!inTree && folder ? (
						<span className="text-muted-foreground ml-1.5 text-[0.6875rem]">{folder}</span>
					) : null}
				</span>
			</button>
			<span className="flex shrink-0 gap-0.5 md:hidden md:group-focus-within/row:flex md:group-hover/row:flex">
				{actions.canOpenFile(change) ? (
					<IconButton label={t("changes.openFile")} onClick={() => actions.onOpenFile(change)}>
						<FileText />
					</IconButton>
				) : null}
				{!actions.readOnly && actions.canDiscard(entry) ? (
					<IconButton label={t("changes.discard")} onClick={() => actions.onDiscard([entry])}>
						<Undo2 />
					</IconButton>
				) : null}
			</span>
			<Counts change={change} />
			<span
				title={change.status}
				className={cn(
					"w-3 shrink-0 text-center font-mono text-xs font-semibold",
					LETTER_TONE[letter],
				)}
			>
				{letter}
			</span>
		</li>
	);
}

/** `+12 −3`, quiet. */
export function Counts({ change }: { change: GitChange }) {
	if (!change.added && !change.removed) return null;
	return (
		<span className="shrink-0 font-mono text-[0.625rem] tabular-nums">
			{change.added ? <span className="text-success">+{change.added}</span> : null}
			{change.removed ? <span className="text-destructive ml-1">−{change.removed}</span> : null}
		</span>
	);
}

/** A tri-state checkbox: staged, partly staged, or not. */
function Checkbox({
	state,
	label,
	disabled,
	onClick,
}: {
	state: Staged;
	label: string;
	disabled?: boolean;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			role="checkbox"
			aria-checked={state === "all" ? true : state === "some" ? "mixed" : false}
			aria-label={label}
			title={label}
			disabled={disabled}
			onClick={(event) => {
				event.stopPropagation();
				onClick();
			}}
			className="focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 disabled:opacity-50 max-md:size-8"
		>
			<span
				className={cn(
					"grid size-3.5 place-items-center rounded-[4px] border transition-colors [&_svg]:size-3 [&_svg]:stroke-[3]",
					state === "none"
						? "border-muted-foreground/50 hover:border-foreground bg-transparent"
						: "border-primary bg-primary text-primary-foreground",
				)}
			>
				{state === "all" ? <Check /> : state === "some" ? <Minus /> : null}
			</span>
		</button>
	);
}

export function Chevron({
	open,
	onClick,
	label,
}: {
	open: boolean;
	onClick: () => void;
	label: string;
}) {
	const { t } = useTranslation("projects");
	return (
		<button
			type="button"
			aria-expanded={open}
			aria-label={open ? t("changes.fold", { name: label }) : t("changes.unfold", { name: label })}
			onClick={onClick}
			className="text-muted-foreground hover:text-foreground grid h-6 w-4 shrink-0 place-items-center outline-none"
		>
			<ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
		</button>
	);
}
