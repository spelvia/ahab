import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import {
	checkRoots,
	getCheckRootsQueryKey,
	getListProblemsQueryKey,
	getListRepositoryActivityQueryKey,
	getTurnDiagnosticsQueryKey,
	listProblems,
	useListProblems,
	useListRepositoryActivity,
	usePinRepository,
	useRunChecks as useGeneratedRunChecks,
	useTurnDiagnostics as useGeneratedTurnDiagnostics,
} from "@/shared/api/generated/tools-problems/tools-problems";

/** One file's stored problems, asked once (file windows, for tools without a live channel). */
export function fetchFileProblems(projectId: string, path: string) {
	return listProblems(projectId, { path });
}

/**
 * A project's problems, all of them: the view filters them itself, so
 * changing a filter is instant and every tool stays listed.
 */
export function useProblems(projectId: string, enabled = true) {
	return useListProblems(projectId, undefined, {
		query: { enabled: enabled && Boolean(projectId), staleTime: 30_000 },
	});
}

/** Runs a project's check commands now; the list follows when they end. */
export function useRunChecks(projectId: string) {
	const refresh = useRefreshProblems();
	return useGeneratedRunChecks({
		mutation: { onSettled: () => refresh(projectId) },
	});
}

/**
 * Where Check project would run, asked when it is pressed: each check's
 * roots with the repository or worktree each is in.
 */
export function useCheckRootsLoader() {
	const client = useQueryClient();
	return useCallback(
		(projectId: string) =>
			client.fetchQuery({
				queryKey: getCheckRootsQueryKey(projectId),
				queryFn: () => checkRoots(projectId),
				staleTime: 30_000,
			}),
		[client],
	);
}

/** A project's used and pinned repositories. */
export function useRepositoryActivity(projectId: string | undefined, enabled = true) {
	return useListRepositoryActivity(projectId ?? "", {
		query: { enabled: enabled && Boolean(projectId), staleTime: 30_000 },
	});
}

/** Pins or unpins a repository or worktree; the list follows. */
export function usePinRepositoryOf(projectId: string | undefined) {
	const client = useQueryClient();
	return usePinRepository({
		mutation: {
			onSuccess: (list) =>
				client.setQueryData(getListRepositoryActivityQueryKey(projectId ?? ""), list),
		},
	});
}

/** Asks again for a project's repositories' use. */
export function useRefreshActivity() {
	const client = useQueryClient();
	return useCallback(
		(projectId: string) =>
			void client.invalidateQueries({
				queryKey: getListRepositoryActivityQueryKey(projectId),
			}),
		[client],
	);
}

/**
 * What a finished turn changed. None (a 404) when it changed nothing, or
 * while the server is still comparing; asked again when it says so.
 */
export function useTurnDiagnostics(chatId: string, turnId: string) {
	return useGeneratedTurnDiagnostics(chatId, turnId, {
		query: { retry: false, staleTime: Number.POSITIVE_INFINITY },
	});
}

/** Asks again for a project's problems and its totals. */
export function useRefreshProblems() {
	const client = useQueryClient();
	return useCallback(
		(projectId: string) => {
			const list = String(getListProblemsQueryKey(projectId)[0]);
			void client.invalidateQueries({
				predicate: (query) => {
					const key = String(query.queryKey[0]);
					return (
						key === list ||
						(key.startsWith("/api/v1/projects/") && key.endsWith("/problems/summary"))
					);
				},
			});
		},
		[client],
	);
}

/** Asks again for a turn's difference, now that the server has it. */
export function useRefreshTurn() {
	const client = useQueryClient();
	return useCallback(
		(chatId: string, turnId: string) =>
			void client.invalidateQueries({ queryKey: getTurnDiagnosticsQueryKey(chatId, turnId) }),
		[client],
	);
}
