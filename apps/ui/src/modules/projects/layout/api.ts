import { replaceEqualDeep, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { problemCode } from "@/shared/api/fetch";
import type { ProjectLayout } from "@/shared/api/generated/ahab.schemas";
import {
	getGetLayoutQueryKey,
	getLayout,
	saveLayout,
	useGetLayout,
} from "@/shared/api/generated/projects-layout/projects-layout";
import {
	type PendingLayout,
	pendingProjectLayout,
	rememberPendingLayout,
} from "@/shared/preferences/store";

/** Phones keep a layout of their own. */
export type ClientClass = "desktop" | "mobile";

/**
 * Where tiles load and save their layout: a project's windows.
 * `layout` is undefined until loaded, null when none is saved yet.
 */
export interface LayoutStore {
	/** Changes with what the store is for, so the tiles start over. */
	key: string;
	layout: { layout: unknown; revision: number } | null | undefined;
	failed: boolean;
	pending?: boolean;
	conflict?: { useSaved: () => void; keepMine: () => void };
	/** Keep an unsent snapshot before the debounced write or page reload. */
	stage?: (layout: Record<string, unknown>, revision: number) => void;
	/** Saves over `revision` and returns the confirmed revision. */
	save: (layout: Record<string, unknown>, revision: number) => Promise<number>;
}

const writes = new Map<string, Promise<number>>();
const pending = new Map<string, PendingLayout>();
const attempted = new Map<string, PendingLayout>();

function pendingLayout(key: string): PendingLayout | undefined {
	const snapshot = pending.get(key) ?? pendingProjectLayout(key);
	if (snapshot) pending.set(key, snapshot);
	return snapshot;
}

function keepLayout(key: string, snapshot: PendingLayout | undefined) {
	if (snapshot) pending.set(key, snapshot);
	else pending.delete(key);
	rememberPendingLayout(key, snapshot);
}

/** Which layout a window keeps: decided once, so a resized window keeps it. */
export function clientClass(): ClientClass {
	return typeof window !== "undefined" && window.matchMedia?.("(max-width: 767px)").matches
		? "mobile"
		: "desktop";
}

/** A project's windows: its chats, files and new sessions, as last arranged. */
export function useProjectLayout(projectId: string, kind: ClientClass): LayoutStore {
	const client = useQueryClient();
	const [restore, setRestore] = useState(0);
	const key = `project:${projectId}:${kind}`;
	const queryKey = getGetLayoutQueryKey(projectId, kind);
	const saved = useGetLayout(projectId, kind, {
		query: { retry: false, staleTime: Number.POSITIVE_INFINITY, refetchOnWindowFocus: false },
	});
	const missing = saved.isError && problemCode(saved.error) === "layout_not_found";
	const draft = pendingLayout(key);
	const save: LayoutStore["save"] = (_layout, revision) => {
		const previous = writes.get(key);
		const write = async () => {
			const snapshot = pendingLayout(key);
			if (!snapshot) return client.getQueryData<ProjectLayout>(queryKey)?.revision ?? revision;
			const lastAttempt = attempted.get(key);
			attempted.set(key, snapshot);
			let result: ProjectLayout;
			try {
				result = await saveLayout(projectId, kind, snapshot, { timeoutMs: 15_000 });
			} catch (error) {
				if (problemCode(error) !== "layout_changed") throw error;
				const current = await getLayout(projectId, kind);
				// A lost response can look like a conflict. Only resume automatically
				// when the server already holds this snapshot or our previous write.
				if (replaceEqualDeep(current.layout, snapshot.layout) === current.layout) {
					result = current;
				} else if (
					lastAttempt &&
					current.revision === lastAttempt.revision + 1 &&
					replaceEqualDeep(current.layout, lastAttempt.layout) === current.layout
				) {
					const retry = { ...snapshot, revision: current.revision };
					attempted.set(key, retry);
					try {
						result = await saveLayout(projectId, kind, retry, { timeoutMs: 15_000 });
					} catch (retryError) {
						if (problemCode(retryError) === "layout_changed") {
							client.setQueryData(queryKey, await getLayout(projectId, kind));
						}
						throw retryError;
					}
				} else {
					attempted.delete(key);
					client.setQueryData(queryKey, current);
					throw error;
				}
			}
			attempted.delete(key);
			await client.cancelQueries({ queryKey, exact: true });
			const latest = pendingLayout(key);
			if (latest === snapshot) keepLayout(key, undefined);
			else if (latest) keepLayout(key, { ...latest, revision: result.revision });
			client.setQueryData(queryKey, result);
			return result.revision;
		};
		const next = previous ? previous.then(write, write) : write();
		writes.set(key, next);
		const cleanup = () => {
			if (writes.get(key) === next) writes.delete(key);
		};
		void next.then(cleanup, cleanup);
		return next;
	};
	const remote = saved.data;
	const conflict =
		draft &&
		remote &&
		remote.revision > draft.revision &&
		replaceEqualDeep(remote.layout, draft.layout) !== remote.layout
			? {
					useSaved: () => {
						keepLayout(key, undefined);
						attempted.delete(key);
						setRestore((count) => count + 1);
					},
					keepMine: () => {
						const newest = pendingLayout(key);
						if (!newest) return;
						keepLayout(key, { ...newest, revision: remote.revision });
						setRestore((count) => count + 1);
					},
				}
			: undefined;
	return {
		key: `${key}:${restore}`,
		layout: draft ?? remote ?? (missing ? null : undefined),
		failed: !draft && !remote && saved.isError && !missing,
		pending: Boolean(draft),
		conflict,
		stage: (layout, revision) => {
			const previous = pendingLayout(key);
			keepLayout(key, { layout, revision: previous?.revision ?? revision });
		},
		save,
	};
}
