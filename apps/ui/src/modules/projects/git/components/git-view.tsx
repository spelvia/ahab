import { cn } from "cn";
import { GitBranch } from "lucide-react";
import { useMemo, useRef, useState } from "react";

import {
	SHOWN,
	WATCHED,
	useGitActions,
	useGitRefresh,
	useGitStatus,
	useGitWatch,
	useIsWatched,
} from "@/modules/projects/git/api";
import { BranchPicker } from "@/modules/projects/git/components/branch-picker";
import { type ChangeActions, Changes, Chevron } from "@/modules/projects/git/components/changes";
import { Conflicts, Operation } from "@/modules/projects/git/components/conflicts";
import { GraphView } from "@/modules/projects/git/components/graph-view";
import { MainButton, RepositoryHeader } from "@/modules/projects/git/components/repository-header";
import { Stashes } from "@/modules/projects/git/components/stashes";
import {
	type Checkout,
	type Entry,
	entries as entriesOf,
	groupCheckouts,
	type Staged,
} from "@/modules/projects/git/model";
import type {
	GitChange,
	GitDiffGroup,
	GitRepository,
	Project,
} from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import { gitTree, rememberGitTree } from "@/shared/preferences/store";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { toast, toastError } from "@/shared/ui/toast";

/** What the Git view opens: a change's diff, or a file of the project. */
export interface GitOpeners {
	onOpenDiff: (change: GitChange, group: GitDiffGroup, commit?: string) => void;
	/** `path` relative to the project. */
	onOpenFile: (path: string) => void;
}

/** A question asked before something that cannot be undone. */
interface Question {
	title: string;
	description: string;
	action: string;
	destructive?: boolean;
	run: () => void;
}

/**
 * The project's Git, as VS Code's Source Control: each repository a
 * section with its branch, sync state and commit box, its changes in one
 * checked list, then the graph of commits and the stashes.
 */
export function GitView({
	project,
	repository,
	repositories,
	onChooseRepository,
	edited,
	onOpenDiff,
	onOpenFile,
	checkoutActions,
}: {
	project: Project;
	/** A repository inside the project's folder; absent for the one it is in. */
	repository?: string;
	/** The repositories inside the folder, when it is in none. */
	repositories?: GitRepository[];
	/** Makes a repository the current one (the status bar's branch, the graph). */
	onChooseRepository?: (path: string) => void;
	/** Files the open session's agents edited, as absolute paths (marked). */
	edited?: readonly string[];
	/**
	 * Buttons for a repository (its folder) or one of its worktrees, when the
	 * project holds several: Pin and Unpin, from language tooling.
	 */
	checkoutActions?: React.ComponentType<{
		projectId: string;
		repository: string;
		worktree: string | null;
	}>;
} & GitOpeners) {
	const { t } = useTranslation("projects");
	const [asTree, setAsTree] = useState(gitTree);
	const [open, setOpen] = useState<ReadonlySet<string>>(
		() => new Set(repository ? [repository] : []),
	);
	const several = Boolean(repository && repositories && repositories.length > 1);
	// Repositories of several worktrees start folded, but for the current
	// checkout's; these were folded or unfolded since.
	const [flipped, setFlipped] = useState<ReadonlySet<string>>(() => new Set());
	const openers = { onOpenDiff, onOpenFile };
	const treeChange = (on: boolean) => {
		rememberGitTree(on);
		setAsTree(on);
	};

	// One checkout of several: folded ones ask Git nothing.
	const item = (checkout: Checkout, nested: boolean, group?: string) => {
		const toggle = () => {
			const opening = !open.has(checkout.path);
			setOpen((set) => flip(set, checkout.path));
			if (opening) onChooseRepository?.(checkout.path);
		};
		const name = nested ? checkout.label : checkout.path;
		const Actions = checkoutActions;
		const extra = Actions ? (
			<Actions
				projectId={project.id}
				repository={group ?? checkout.path}
				worktree={group ? checkout.path : null}
			/>
		) : null;
		// Folded ones ask Git nothing: a long list costs nothing.
		return open.has(checkout.path) ? (
			<RepositorySection
				key={checkout.path}
				project={project}
				repository={checkout.path}
				name={name}
				open
				current={checkout.path === repository}
				onToggle={toggle}
				onChoose={() => onChooseRepository?.(checkout.path)}
				asTree={asTree}
				onTreeChange={treeChange}
				openers={openers}
				extra={extra}
			/>
		) : (
			<FoldedRepository
				key={checkout.path}
				name={name}
				branch={checkout.branch}
				current={checkout.path === repository}
				onToggle={toggle}
				extra={extra}
			/>
		);
	};

	return (
		<div className="flex min-h-0 flex-1 flex-col">
			{/* The repositories scroll; the graph below keeps its own pane. */}
			<div className="flex min-h-0 flex-[0_1_auto] flex-col gap-1 overflow-auto pb-2">
				{several && repositories ? (
					groupCheckouts(repositories).map((group) => {
						if (group.checkouts.length === 1 && group.checkouts[0]?.path === group.repository)
							return item(group.checkouts[0], false);
						const holdsCurrent = group.checkouts.some((c) => c.path === repository);
						const shut = holdsCurrent === flipped.has(group.repository);
						const count = group.checkouts.length;
						return (
							<section
								key={group.repository}
								aria-label={t("gitView.repositoryNamed", { name: group.repository })}
								className="flex flex-col"
							>
								<div className="flex h-8 min-w-0 items-center gap-0.5 pr-1.5 pl-1">
									<Chevron
										open={!shut}
										onClick={() => setFlipped((set) => flip(set, group.repository))}
										label={group.repository}
									/>
									<button
										type="button"
										onClick={() => setFlipped((set) => flip(set, group.repository))}
										title={group.repository}
										className="flex min-w-0 flex-1 items-baseline gap-1.5 pl-0.5 text-left"
									>
										<PathName path={group.repository} />
									</button>
									<span className="text-muted-foreground shrink-0 pl-1 text-[0.6875rem]">
										{t("gitView.worktrees", { count })}
									</span>
									{checkoutActions ? (
										<GroupActions
											Actions={checkoutActions}
											projectId={project.id}
											repository={group.repository}
										/>
									) : null}
								</div>
								{shut ? null : (
									<div className="border-border/60 ml-3 border-l pl-1">
										{group.checkouts.map((checkout) => item(checkout, true, group.repository))}
									</div>
								)}
							</section>
						);
					})
				) : (
					<RepositorySection
						project={project}
						repository={repository}
						name={repository}
						open
						current
						asTree={asTree}
						onTreeChange={treeChange}
						edited={edited}
						openers={openers}
					/>
				)}
				<Known project={project} repository={repository}>
					<Stashes
						projectId={project.id}
						repository={repository}
						readOnly={project.mode === "readOnly"}
					/>
				</Known>
			</div>
			<Known project={project} repository={repository}>
				<GraphView
					key={repository ?? ""}
					projectId={project.id}
					repository={repository}
					name={several ? repository : undefined}
					onOpenDiff={onOpenDiff}
				/>
			</Known>
		</div>
	);
}

/** Shows its children once the repository is known to be one. */
function Known({
	project,
	repository,
	children,
}: {
	project: Project;
	repository?: string;
	children: React.ReactNode;
}) {
	const status = useGitStatus(project.id, repository, true, false);
	return status.data?.repository ? children : null;
}

/**
 * A repository: its header (name, branch, ↓↑, commit, refresh, more), and
 * when open, its commit box and its changes.
 */
function RepositorySection({
	project,
	repository,
	name,
	open,
	current,
	onToggle,
	onChoose,
	asTree,
	onTreeChange,
	edited,
	openers,
	extra,
}: {
	project: Project;
	repository?: string;
	edited?: readonly string[];
	/** Shown when the project has several repositories. */
	name?: string;
	open: boolean;
	current: boolean;
	/** Folds and unfolds it; absent when it is the only one. */
	onToggle?: () => void;
	onChoose?: () => void;
	asTree: boolean;
	onTreeChange: (tree: boolean) => void;
	openers: GitOpeners;
	extra?: React.ReactNode;
}) {
	const { t } = useTranslation("projects");
	const id = project.id;
	// Its machine tells of changes while it is open; asked often only when it cannot.
	useGitWatch(id, repository, open);
	const isWatched = useIsWatched(id, repository);
	const status = useGitStatus(id, repository, true, open ? (isWatched ? WATCHED : SHOWN) : false);
	const actions = useGitActions(id);
	const refresh = useGitRefresh(id);
	const repo = repository ? { repository } : undefined;
	const readOnly = project.mode === "readOnly";
	const [message, setMessage] = useState("");
	const [question, setQuestion] = useState<Question | null>(null);
	const [picking, setPicking] = useState(false);
	const [initializing, setInitializing] = useState(false);
	const box = useRef<HTMLTextAreaElement>(null);
	const data = status.data;
	const list = useMemo(() => (data ? entriesOf(data) : ([] as Entry<GitChange>[])), [data]);
	// The session's edited files, as paths in this repository.
	const ours = useMemo(() => {
		if (!data?.repository || !edited?.length) return new Set<string>();
		const folder = repository ? `${project.path}/${repository}` : project.path;
		const root = data.prefix ? folder.slice(0, -data.prefix.length).replace(/\/$/, "") : folder;
		return new Set(
			edited
				.filter((path) => path.startsWith(`${root}/`))
				.map((path) => path.slice(root.length + 1)),
		);
	}, [edited, data, repository, project.path]);

	if (status.isPending)
		return (
			<p className="text-muted-foreground px-3 py-1 text-sm">
				{t("state.loading", { ns: "common" })}
			</p>
		);
	if (status.isError)
		return (
			<p className="text-muted-foreground px-3 py-1 text-sm">
				{explain(status.error, t("gitView.unreadable"))}
			</p>
		);
	if (!data?.repository)
		return (
			<div className="px-3 py-1 text-sm">
				<p className="text-muted-foreground mb-1">{t("gitView.notRepository.message")}</p>
				<p className="text-muted-foreground mb-3 text-xs">{t("gitView.notRepository.hint")}</p>
				<Button
					size="sm"
					variant="outline"
					disabled={readOnly || actions.init.isPending}
					onClick={() => setInitializing(true)}
				>
					{t("gitView.notRepository.initialize")}
				</Button>
				<ConfirmDialog
					open={initializing}
					onOpenChange={setInitializing}
					title={t("gitView.initialize.title")}
					description={t("gitView.initialize.description", {
						path: project.path,
						machine: project.machineName,
					})}
					action={t("gitView.initialize.action")}
					onConfirm={() => {
						setInitializing(false);
						actions.init.mutate({ id }, { onError: toastError });
					}}
				/>
			</div>
		);

	// Paths are the repository's; inside the project's folder they open as
	// files of the project.
	const inProject = (path: string) =>
		Boolean(repository) || !data.prefix || path.startsWith(data.prefix);
	const relative = (path: string) =>
		repository ? `${repository}/${path}` : data.prefix ? path.slice(data.prefix.length) : path;
	const nothingStaged = data.staged.length === 0;
	const anything = list.length > 0;
	const busy =
		actions.fetch.isPending ||
		actions.pull.isPending ||
		actions.push.isPending ||
		actions.commit.isPending ||
		actions.stage.isPending ||
		actions.unstage.isPending;

	const push = (done: string = t("gitView.toast.pushed")) =>
		actions.push.mutate(
			{ id, params: repo },
			{ onSuccess: () => toast(done), onError: toastError },
		);
	// Sync: pull, then push; a branch without an upstream is published.
	const sync = () => {
		if (!data.upstream) return push(t("gitView.toast.published"));
		actions.pull.mutate(
			{ id, params: repo },
			{ onSuccess: () => push(t("gitView.toast.synced")), onError: toastError },
		);
	};
	const remote = (kind: "fetch" | "pull") =>
		actions[kind].mutate(
			{ id, params: repo },
			{
				onSuccess: () =>
					toast(kind === "fetch" ? t("gitView.toast.fetched") : t("gitView.toast.pulled")),
				onError: toastError,
			},
		);
	const commit = (then?: "push" | "sync", amend = false) => {
		const run = () =>
			actions.commit.mutate(
				{ id, params: repo, data: { message, amend } },
				{
					onSuccess: () => {
						setMessage("");
						if (then === "push")
							push(
								amend ? t("gitView.toast.amendedAndPushed") : t("gitView.toast.committedAndPushed"),
							);
						else if (then === "sync") sync();
						else toast(amend ? t("gitView.toast.amended") : t("gitView.toast.committed"));
					},
					onError: toastError,
				},
			);
		if (nothingStaged && !amend) {
			actions.stage.mutate(
				{ id, params: repo, data: { all: true } },
				{ onSuccess: run, onError: toastError },
			);
		} else run();
	};
	const amend = () => {
		// A pushed commit changed here differs from the remote's.
		if (data.upstream && data.ahead === 0)
			setQuestion({
				title: t("gitView.amendPushed.title"),
				description: t("gitView.amendPushed.description", {
					branch: data.branch ?? "HEAD",
					upstream: data.upstream,
				}),
				action: t("gitView.amendPushed.action"),
				run: () => commit(undefined, true),
			});
		else commit(undefined, true);
	};

	// The main button follows the situation, as in VS Code.
	const mode: "commit" | "publish" | "sync" = anything
		? "commit"
		: data.branch && !data.upstream
			? "publish"
			: data.ahead || data.behind
				? "sync"
				: "commit";
	const canCommit = Boolean(message.trim()) && anything && !actions.commit.isPending;

	const toggle = (paths: string[], staged: Staged) => {
		const mutation = staged === "all" ? actions.unstage : actions.stage;
		mutation.mutate({ id, params: repo, data: { paths } }, { onError: toastError });
	};
	const changeActions: ChangeActions = {
		readOnly,
		onToggle: toggle,
		onOpen: (entry) => openers.onOpenDiff(entry.change, entry.group),
		canOpenFile: (change) => inProject(change.path) && change.status !== "deleted",
		onOpenFile: (change) => openers.onOpenFile(relative(change.path)),
		canDiscard: (entry) => entry.staged !== "all" && inProject(entry.path),
		onDiscard: (items) => {
			const one = items.length === 1 ? items[0] : undefined;
			setQuestion({
				title: one
					? t("gitView.discard.titleOne", { path: one.path })
					: t("gitView.discard.titleMany", { count: items.length }),
				description: one
					? one.group === "untracked"
						? t("gitView.discard.untracked")
						: t("gitView.discard.tracked")
					: t("gitView.discard.many"),
				action: t("gitView.discard.action"),
				destructive: true,
				run: () =>
					actions.discard.mutate(
						{ id, params: repo, data: { paths: items.map((item) => item.path) } },
						{ onError: toastError },
					),
			});
		},
		inProject,
	};

	return (
		<section
			aria-label={name ? t("gitView.repositoryNamed", { name }) : t("gitView.repository")}
			className="flex flex-col"
		>
			<RepositoryHeader
				data={data}
				name={name}
				open={open}
				current={current}
				onToggle={onToggle}
				busy={busy}
				readOnly={readOnly}
				anything={anything}
				onBranch={() => {
					onChoose?.();
					setPicking(true);
				}}
				onSync={sync}
				onCommit={() => {
					if (!open) onToggle?.();
					if (canCommit) commit();
					else box.current?.focus();
				}}
				onRefresh={refresh}
				extra={extra}
				onPull={() => remote("pull")}
				onPush={() =>
					push(data.upstream ? t("gitView.toast.pushed") : t("gitView.toast.published"))
				}
				onFetch={() => remote("fetch")}
				onStash={() =>
					actions.stash.mutate(
						{ id, params: repo, data: { untracked: true } },
						{ onSuccess: () => toast(t("gitView.toast.stashed")), onError: toastError },
					)
				}
			/>
			{open ? (
				<div className="flex flex-col gap-1.5 pt-0.5">
					{data.operation ? (
						<Operation
							status={data}
							onContinue={() =>
								actions.continueOperation.mutate({ id, params: repo }, { onError: toastError })
							}
							onAbort={() => actions.abort.mutate({ id, params: repo }, { onError: toastError })}
						/>
					) : null}

					{readOnly ? null : (
						<div className="px-3">
							<textarea
								ref={box}
								aria-label={t("gitView.commitMessage")}
								value={message}
								onChange={(event) => setMessage(event.target.value)}
								onKeyDown={(event) => {
									if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
										event.preventDefault();
										if (canCommit) commit();
									}
								}}
								placeholder={t("gitView.commitPlaceholder", { branch: data.branch ?? "HEAD" })}
								rows={Math.min(6, Math.max(2, message.split("\n").length))}
								className="border-input bg-card focus-visible:ring-ring/40 w-full resize-none rounded-md border px-2.5 py-1.5 text-[0.8125rem] outline-none focus-visible:ring-2"
							/>
							<MainButton
								mode={mode}
								status={data}
								all={nothingStaged && anything}
								disabled={
									mode === "commit" ? !canCommit : actions.push.isPending || actions.pull.isPending
								}
								canCommit={canCommit}
								hasCommits={Boolean(data.head)}
								onCommit={() => commit()}
								onPublish={() => push(t("gitView.toast.published"))}
								onSync={sync}
								onCommitPush={() => commit("push")}
								onCommitSync={() => commit("sync")}
								onAmend={amend}
							/>
						</div>
					)}

					<Conflicts
						changes={data.conflicts}
						readOnly={readOnly}
						onOpen={(change) => openers.onOpenDiff(change, "conflicts")}
						onResolve={(change) =>
							actions.resolve.mutate(
								{ id, params: repo, data: { paths: [change.path] } },
								{ onError: toastError },
							)
						}
					/>
					<Changes
						entries={list}
						edited={ours}
						asTree={asTree}
						onTreeChange={onTreeChange}
						actions={changeActions}
					/>
					{list.length + data.conflicts.length === 0 ? (
						<p className="text-muted-foreground px-3 text-xs">{t("gitView.noChanges")}</p>
					) : null}
				</div>
			) : null}

			<BranchPicker
				project={project}
				repository={repository}
				open={picking}
				onOpenChange={setPicking}
			/>
			<ConfirmDialog
				open={question !== null}
				onOpenChange={(next) => !next && setQuestion(null)}
				title={question?.title ?? ""}
				description={question?.description ?? ""}
				action={question?.action ?? t("gitView.confirm")}
				destructive={question?.destructive}
				onConfirm={() => {
					question?.run();
					setQuestion(null);
				}}
			/>
		</section>
	);
}

/** A set with `item` added, or taken out if it was in. */
function flip(set: ReadonlySet<string>, item: string): ReadonlySet<string> {
	const next = new Set(set);
	if (next.has(item)) next.delete(item);
	else next.add(item);
	return next;
}

/** A repository group's own buttons (for all its worktrees). */
function GroupActions({
	Actions,
	projectId,
	repository,
}: {
	Actions: React.ComponentType<{
		projectId: string;
		repository: string;
		worktree: string | null;
	}>;
	projectId: string;
	repository: string;
}) {
	return <Actions projectId={projectId} repository={repository} worktree={null} />;
}

/** A folded repository: its name and branch, from the listing alone. */
function FoldedRepository({
	name,
	branch,
	current,
	onToggle,
	extra,
}: {
	name: string;
	branch?: string | null;
	current: boolean;
	onToggle: () => void;
	extra?: React.ReactNode;
}) {
	const { t } = useTranslation("projects");
	return (
		<section aria-label={t("gitView.repositoryNamed", { name })} className="flex flex-col">
			<div className="flex h-8 min-w-0 items-center gap-0.5 pr-1.5 pl-1">
				<Chevron open={false} onClick={onToggle} label={name} />
				<button
					type="button"
					onClick={onToggle}
					title={name}
					className={cn(
						"flex min-w-0 flex-1 items-center gap-2 pl-0.5 text-left",
						current ? "text-heading" : "text-foreground",
					)}
				>
					<span className="flex max-w-[65%] min-w-0 shrink-0 items-baseline gap-1.5">
						<PathName path={name} />
					</span>
					<span className="text-muted-foreground flex min-w-0 items-center gap-1 text-xs">
						<GitBranch className="size-3.5 shrink-0" />
						<span className="truncate">{branch ?? t("gitView.detached")}</span>
					</span>
				</button>
				{extra}
			</div>
		</section>
	);
}

/** A path as its last part, then (quieter) the folders it is in. */
function PathName({ path }: { path: string }) {
	const cut = path.lastIndexOf("/");
	return (
		<>
			<span className="shrink-0 truncate text-[0.8125rem] font-semibold">
				{path.slice(cut + 1)}
			</span>
			{cut > 0 ? (
				<span className="text-muted-foreground min-w-0 truncate text-[0.6875rem]">
					{path.slice(0, cut)}
				</span>
			) : null}
		</>
	);
}
