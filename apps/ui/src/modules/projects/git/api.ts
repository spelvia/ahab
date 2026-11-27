import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useEffectEvent, useSyncExternalStore } from "react";

import type { GitDiffParams } from "@/shared/api/generated/ahab.schemas";
import {
	useGitAbort,
	useGitBranches as useGeneratedBranches,
	gitStatus,
	gitRepositories,
	getGitLogQueryKey,
	gitLog,
	useGitCommit,
	useGitContinue,
	useGitCreateBranch,
	useGitDeleteBranch,
	useGitDiff as useGeneratedDiff,
	useGitDiscard,
	useGitFetch,
	useGitInit,
	useGitPull,
	useGitPush,
	useGitRenameBranch,
	useGitRepositories,
	useGitResolve,
	useGitShow as useGeneratedShow,
	useGitStage,
	useGitStagePart,
	useGitStash,
	useGitStashApply,
	useGitStashDrop,
	useGitStashes as useGeneratedStashes,
	useGitStatus as useGeneratedStatus,
	useGitSwitch,
	useGitUnstage,
} from "@/shared/api/generated/projects-git/projects-git";
import { useInvalidate } from "@/shared/api/invalidate";
import { onResume, retryDelay } from "@/shared/api/resume";

/**
 * Where the project's Git is: the repository its folder is in, or those
 * inside it (`ahab/` in a folder that holds a project's repositories).
 */
export function useRepositories(projectId: string | undefined, enabled = true) {
	return useGitRepositories(projectId ?? "", {
		query: { enabled: enabled && Boolean(projectId), staleTime: 30_000, retry: false },
	});
}

/** How often Git's status is asked again: while the Git view shows it, and otherwise. */
export const SHOWN = 4_000;
export const GLANCE = 15_000;
/** ...and while its machine watches it and tells of changes: only in case one is missed. */
export const WATCHED = 60_000;

/** The checkouts their machines watch right now, as `project|repository`. */
const watched = new Map<string, number>();
const listeners = new Set<() => void>();
const keyOf = (projectId: string, repository?: string) => `${projectId}|${repository ?? ""}`;

function setWatched(key: string, on: boolean) {
	const count = (watched.get(key) ?? 0) + (on ? 1 : -1);
	if (count > 0) watched.set(key, count);
	else watched.delete(key);
	for (const listener of listeners) listener();
}

/** Whether the checkout's machine tells of its changes now (so asking often is not needed). */
export function useIsWatched(projectId: string | undefined, repository?: string) {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		() => Boolean(projectId) && watched.has(keyOf(projectId ?? "", repository)),
	);
}

/**
 * Has the checkout's machine watch it while `enabled`, asking for its
 * status, diffs, branches and stashes again after each change. While the
 * watch is up, the status is asked only every `WATCHED`; when it drops (or
 * the machine cannot watch), the usual polling takes over, and the watch
 * is tried again.
 */
export function useGitWatch(projectId: string, repository: string | undefined, enabled: boolean) {
	const refresh = useInvalidate(
		...["status", "diff", "branches", "stashes"].map(
			(part) => `/projects/${projectId}/git/${part}`,
		),
	);
	const changed = useEffectEvent(refresh);

	useEffect(() => {
		if (!enabled || typeof EventSource === "undefined") return;
		const key = keyOf(projectId, repository);
		let source: EventSource | undefined;
		let retry: ReturnType<typeof setTimeout> | undefined;
		let on = false;
		let stopped = false;
		let failures = 0;
		/** The machine cannot watch it: polling it is, until the view opens again. */
		let unwatched = false;
		const mark = (now: boolean) => {
			if (now !== on) setWatched(key, now);
			on = now;
		};
		const connect = () => {
			if (stopped) return;
			const query = repository ? `?repository=${encodeURIComponent(repository)}` : "";
			source = new EventSource(`/api/v1/projects/${projectId}/git/watch${query}`);
			source.addEventListener("message", (message) => {
				const frame = JSON.parse(String(message.data)) as { type: string };
				if (frame.type === "watching") {
					failures = 0;
					mark(true);
					// Whatever changed before the watch began.
					changed();
				} else if (frame.type === "changed") {
					changed();
				} else if (frame.type === "unwatched") {
					// It cannot be watched: polling it is, until the view opens again.
					unwatched = true;
					mark(false);
					source?.close();
				}
			});
			source.addEventListener("error", () => {
				mark(false);
				source?.close();
				retry = setTimeout(connect, retryDelay(failures++));
			});
		};
		connect();
		// Back on the page: a broken (or possibly dead) watch is opened again now.
		const stopResume = onResume((stale) => {
			if (stopped || unwatched) return;
			if (!stale && source && source.readyState !== EventSource.CLOSED) return;
			clearTimeout(retry);
			failures = 0;
			source?.close();
			connect();
		});
		return () => {
			stopped = true;
			stopResume();
			clearTimeout(retry);
			source?.close();
			mark(false);
		};
	}, [projectId, repository, enabled]);
}

/**
 * The project's Git status, asked again every `every` milliseconds (or
 * only after an operation, with `false`). Every user of the same status
 * shares one request, at the shortest of their intervals.
 */
export function useGitStatus(
	projectId: string | undefined,
	repository?: string,
	enabled = true,
	every: number | false = SHOWN,
) {
	return useGeneratedStatus(projectId ?? "", repository ? { repository } : undefined, {
		query: {
			enabled: enabled && Boolean(projectId),
			refetchInterval: every,
			retry: false,
		},
	});
}

export function useBranches(projectId: string, repository?: string, enabled = true) {
	return useGeneratedBranches(projectId, repository ? { repository } : undefined, {
		query: { enabled, retry: false },
	});
}

const PAGE = 100;

/**
 * The graph's commits, a page at a time as it is scrolled: newest first,
 * every branch or only the current one.
 */
export function useGraphLog(
	projectId: string,
	repository: string | undefined,
	all: boolean,
	enabled = true,
) {
	return useInfiniteQuery({
		queryKey: [...getGitLogQueryKey(projectId, { repository, all }), "pages"],
		queryFn: ({ pageParam, signal }) =>
			gitLog(projectId, { repository, all, skip: pageParam, limit: PAGE }, { signal }),
		initialPageParam: 0,
		getNextPageParam: (last, pages) => (last.more ? pages.length * PAGE : undefined),
		enabled,
		retry: false,
	});
}

/** A commit's message and files (`git show`). */
export function useCommitDetail(
	projectId: string,
	repository: string | undefined,
	hash: string | undefined,
) {
	return useGeneratedShow(projectId, hash ?? "", repository ? { repository } : undefined, {
		query: { enabled: Boolean(hash), staleTime: Number.POSITIVE_INFINITY },
	});
}

export function useStashes(projectId: string, repository?: string, enabled = true) {
	return useGeneratedStashes(projectId, repository ? { repository } : undefined, {
		query: { enabled, retry: false },
	});
}

/**
 * A file's two sides; asked again while the window shows changes (rarely
 * while the machine watches the checkout, which says when to).
 */
export function useDiff(projectId: string, params: GitDiffParams) {
	const live = params.group !== "commit";
	const isWatched = useIsWatched(projectId, params.repository);
	return useGeneratedDiff(projectId, params, {
		query: { refetchInterval: live ? (isWatched ? WATCHED : SHOWN) : false, retry: false },
	});
}

/** After any change, everything about this project's Git is asked again. */
export function useGitRefresh(projectId: string) {
	return useInvalidate(`/projects/${projectId}/git/`);
}

/** Every operation that changes the repository, each refreshing the rest. */
export function useGitActions(projectId: string) {
	const refresh = useGitRefresh(projectId);
	const options = { mutation: { onSettled: refresh } };
	return {
		stage: useGitStage(options),
		stagePart: useGitStagePart(options),
		unstage: useGitUnstage(options),
		discard: useGitDiscard(options),
		commit: useGitCommit(options),
		switchBranch: useGitSwitch(options),
		createBranch: useGitCreateBranch(options),
		renameBranch: useGitRenameBranch(options),
		deleteBranch: useGitDeleteBranch(options),
		fetch: useGitFetch(options),
		pull: useGitPull(options),
		push: useGitPush(options),
		stash: useGitStash(options),
		stashApply: useGitStashApply(options),
		stashDrop: useGitStashDrop(options),
		resolve: useGitResolve(options),
		continueOperation: useGitContinue(options),
		abort: useGitAbort(options),
		init: useGitInit(options),
	};
}

/** Resolve the actual checkout and index/worktree side when opening a transcript file's live diff. */
export async function projectFileDiff(projectId: string, file: string) {
	const repos = await gitRepositories(projectId);
	const repository = repos.repositories
		.filter((repo) => file.startsWith(repo.path + "/"))
		.toSorted((a, b) => b.path.length - a.path.length)[0]?.path;
	if (!repository && !repos.inside) return undefined;
	const status = await gitStatus(projectId, repository ? { repository } : undefined);
	const path = repository ? file.slice(repository.length + 1) : (status.prefix ?? "") + file;
	for (const group of ["conflicts", "changes", "untracked", "staged"] as const) {
		const change = status[group].find((entry) => entry.path === path || entry.oldPath === path);
		if (change)
			return {
				projectId,
				repository,
				prefix: status.prefix ?? "",
				path: change.path,
				oldPath: change.oldPath ?? undefined,
				group,
			};
	}
	return undefined;
}
