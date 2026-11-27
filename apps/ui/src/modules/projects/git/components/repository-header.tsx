import { cn } from "cn";
import {
	Archive,
	ArrowDown,
	ArrowUp,
	ArrowUpDown,
	Check,
	ChevronDown,
	CloudUpload,
	GitBranch,
	MoreHorizontal,
	PencilLine,
	RefreshCw,
} from "lucide-react";
import type * as React from "react";

import { Chevron } from "@/modules/projects/git/components/changes";
import type { GitStatus } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { IconButton } from "@/shared/ui/icon-button";

/**
 * A repository's header: its name (when there are several), the branch
 * (opening the picker), how far it is ahead and behind (syncing), and
 * commit, refresh and more.
 */
export function RepositoryHeader({
	data,
	name,
	open,
	current,
	onToggle,
	busy,
	readOnly,
	anything,
	onBranch,
	onSync,
	onCommit,
	onRefresh,
	onPull,
	onPush,
	onFetch,
	onStash,
	extra,
}: {
	data: GitStatus;
	name?: string;
	open: boolean;
	current: boolean;
	onToggle?: () => void;
	busy: boolean;
	readOnly: boolean;
	anything: boolean;
	onBranch: () => void;
	onSync: () => void;
	onCommit: () => void;
	onRefresh: () => void;
	onPull: () => void;
	onPush: () => void;
	onFetch: () => void;
	onStash: () => void;
	/** More buttons, beside its own (Pin, from language tooling). */
	extra?: React.ReactNode;
}) {
	const { t } = useTranslation("projects");
	const changed = data.staged.length + data.changes.length + data.untracked.length;
	const branch =
		data.branch ??
		(data.head
			? t("repositoryHeader.detachedAt", { head: data.head })
			: t("repositoryHeader.noCommits"));
	return (
		<div
			className={cn(
				"group/repo flex h-8 min-w-0 items-center gap-0.5 pr-1.5",
				onToggle ? "pl-1" : "pl-2",
			)}
		>
			{onToggle && name ? (
				<>
					<Chevron open={open} onClick={onToggle} label={name} />
					<button
						type="button"
						onClick={onToggle}
						title={name}
						className={cn(
							"max-w-[55%] shrink-0 truncate pr-1 pl-0.5 text-left text-[0.8125rem] font-semibold",
							current ? "text-heading" : "text-foreground",
						)}
					>
						{name}
					</button>
				</>
			) : null}
			<button
				type="button"
				onClick={onBranch}
				title={t("repositoryHeader.switchBranchTitle", { branch })}
				aria-label={t("repositoryHeader.switchBranchLabel", { branch })}
				className={cn(
					"hover:bg-accent focus-visible:ring-ring flex h-6 min-w-0 shrink items-center gap-1 rounded-md px-1.5 text-xs outline-none focus-visible:ring-2",
					onToggle ? "text-muted-foreground" : "text-heading text-[0.8125rem] font-medium",
				)}
			>
				<GitBranch className="size-3.5 shrink-0" />
				<span className="truncate">{branch}</span>
			</button>
			{data.upstream && (data.ahead || data.behind || !onToggle) ? (
				<button
					type="button"
					onClick={onSync}
					disabled={readOnly}
					title={t("repositoryHeader.syncTitle", {
						upstream: data.upstream,
						behind: data.behind,
						ahead: data.ahead,
					})}
					aria-label={t("repositoryHeader.syncLabel", { behind: data.behind, ahead: data.ahead })}
					className="text-muted-foreground hover:text-foreground hover:bg-accent flex h-6 shrink-0 items-center gap-1 rounded-md px-1 font-mono text-[0.6875rem] tabular-nums outline-none disabled:opacity-60"
				>
					<span className="flex items-center">
						<ArrowDown className="size-3" />
						{data.behind}
					</span>
					<span className="flex items-center">
						<ArrowUp className="size-3" />
						{data.ahead}
					</span>
				</button>
			) : !data.upstream && data.branch ? (
				<span
					title={t("repositoryHeader.localTitle")}
					className="text-muted-foreground shrink-0 px-1 text-[0.6875rem]"
				>
					{t("repositoryHeader.local")}
				</span>
			) : null}
			{!open && changed ? (
				<span className="bg-muted text-muted-foreground shrink-0 rounded-full px-1.5 text-[0.625rem] font-medium">
					{changed}
				</span>
			) : null}
			<span className="min-w-1 flex-1" />
			{extra}
			{busy ? (
				<RefreshCw
					aria-label={t("repositoryHeader.working")}
					className="text-muted-foreground mr-1 size-3.5 shrink-0 animate-spin"
				/>
			) : null}
			<span className="flex shrink-0 items-center gap-0.5 md:opacity-0 md:group-focus-within/repo:opacity-100 md:group-hover/repo:opacity-100">
				{readOnly ? null : (
					<IconButton
						label={t("repositoryHeader.commitChanges")}
						onClick={onCommit}
						disabled={!anything}
					>
						<Check />
					</IconButton>
				)}
				<IconButton label={t("repositoryHeader.refresh")} onClick={onRefresh}>
					<RefreshCw />
				</IconButton>
			</span>
			<DropdownMenu>
				<DropdownMenuTrigger
					aria-label={t("repositoryHeader.more")}
					className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-6 shrink-0 place-items-center rounded outline-none"
				>
					<MoreHorizontal className="size-3.5" />
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<DropdownMenuItem onSelect={onSync} disabled={readOnly}>
						<ArrowUpDown />
						{data.upstream ? t("repositoryHeader.menu.sync") : t("repositoryHeader.menu.publish")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onPull} disabled={readOnly || !data.upstream}>
						<ArrowDown />
						{t("repositoryHeader.menu.pull")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onPush} disabled={readOnly}>
						<ArrowUp />
						{t("repositoryHeader.menu.push")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onFetch}>
						<RefreshCw />
						{t("repositoryHeader.menu.fetch")}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem onSelect={onBranch}>
						<GitBranch />
						{t("repositoryHeader.menu.switchBranch")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onStash} disabled={readOnly || !anything}>
						<Archive />
						{t("repositoryHeader.menu.stash")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}

/**
 * The primary action, following the situation: Commit with changes,
 * Publish branch when it is on no remote, Sync when it is ahead or behind.
 * Its ▾ has the rest.
 */
export function MainButton({
	mode,
	status,
	all,
	disabled,
	canCommit,
	hasCommits,
	onCommit,
	onPublish,
	onSync,
	onCommitPush,
	onCommitSync,
	onAmend,
}: {
	mode: "commit" | "publish" | "sync";
	status: GitStatus;
	/** Commits every change: nothing is staged. */
	all: boolean;
	disabled: boolean;
	canCommit: boolean;
	hasCommits: boolean;
	onCommit: () => void;
	onPublish: () => void;
	onSync: () => void;
	onCommitPush: () => void;
	onCommitSync: () => void;
	onAmend: () => void;
}) {
	const { t } = useTranslation("projects");
	const label = all ? t("mainButton.commitAll") : t("mainButton.commit");
	return (
		<div className="mt-1.5 flex">
			<Button
				size="sm"
				className="min-w-0 flex-1 rounded-r-none"
				disabled={disabled}
				onClick={mode === "commit" ? onCommit : mode === "publish" ? onPublish : onSync}
			>
				{mode === "commit" ? (
					<>
						<Check />
						{label}
					</>
				) : mode === "publish" ? (
					<>
						<CloudUpload />
						{t("mainButton.publish")}
					</>
				) : (
					<>
						<ArrowUpDown />
						{t("mainButton.sync")}
						<span className="font-mono text-xs opacity-80">
							{status.behind ? `↓${status.behind}` : ""}
							{status.ahead ? ` ↑${status.ahead}` : ""}
						</span>
					</>
				)}
			</Button>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						size="sm"
						aria-label={t("mainButton.more")}
						className="border-primary-foreground/25 rounded-l-none border-l px-1.5"
					>
						<ChevronDown />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<DropdownMenuItem onSelect={onCommit} disabled={!canCommit}>
						<Check />
						{label}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onCommitPush} disabled={!canCommit}>
						<ArrowUp />
						{all ? t("mainButton.commitAllAndPush") : t("mainButton.commitAndPush")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={onCommitSync} disabled={!canCommit || !status.upstream}>
						<ArrowUpDown />
						{all ? t("mainButton.commitAllAndSync") : t("mainButton.commitAndSync")}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem onSelect={onAmend} disabled={!hasCommits}>
						<PencilLine />
						{t("mainButton.amend")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
