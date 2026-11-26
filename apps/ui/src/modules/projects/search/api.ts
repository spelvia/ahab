import { useEffect, useReducer } from "react";

import { ApiError } from "@/shared/api/fetch";
import type {
	Problem,
	SearchFile,
	SearchFrame,
	SearchQuery,
} from "@/shared/api/generated/ahab.schemas";
import {
	searchFiles,
	useFileIndex as useGeneratedFileIndex,
} from "@/shared/api/generated/projects-search/projects-search";

/** A search's results so far. */
interface SearchProgress {
	files: SearchFile[];
	/** Matches so far. */
	total: number;
	/** Why it stopped early (`matches`, `time`), once done. */
	cut?: string | null;
	/** Every result is in. */
	done: boolean;
}

interface State {
	data?: SearchProgress;
	error?: unknown;
	searching: boolean;
	/** The next frame starts new results (the shown ones are the last search's). */
	fresh: boolean;
}

type Action =
	| { type: "start" }
	| { type: "frame"; frame: SearchFrame }
	| { type: "failed"; error: unknown }
	| { type: "idle" };

const idle: State = { searching: false, fresh: true };

export function reduce(state: State, action: Action): State {
	switch (action.type) {
		case "start":
			// The last results stay shown until the new ones begin.
			return { data: state.data, searching: true, fresh: true };
		case "idle":
			return idle;
		case "failed":
			return { error: action.error, searching: false, fresh: true };
		case "frame": {
			const { frame } = action;
			const base: SearchProgress =
				state.fresh || !state.data ? { files: [], total: 0, done: false } : state.data;
			const data: SearchProgress =
				frame.type === "files"
					? {
							...base,
							files: [...base.files, ...frame.files],
							total: base.total + frame.files.reduce((sum, file) => sum + file.matches.length, 0),
						}
					: { ...base, total: frame.total, cut: frame.cut, done: true };
			return { data, searching: !data.done, fresh: false };
		}
	}
}

/** The lines of a streamed answer, each as it arrives. */
async function* lines(body: ReadableStream<Uint8Array>): AsyncGenerator<string> {
	const reader = body.getReader();
	const decoder = new TextDecoder();
	let buffer = "";
	for (;;) {
		// Each read waits for the next part of the answer, in order.
		// oxlint-disable-next-line no-await-in-loop
		const { value, done } = await reader.read();
		if (done) break;
		buffer += decoder.decode(value, { stream: true });
		let end = buffer.indexOf("\n");
		while (end >= 0) {
			const line = buffer.slice(0, end).trim();
			buffer = buffer.slice(end + 1);
			if (line) yield line;
			end = buffer.indexOf("\n");
		}
	}
	if (buffer.trim()) yield buffer.trim();
}

/** Starts a streamed search, answering its lines; throws the API's error. */
async function stream(projectId: string, body: string, signal: AbortSignal) {
	const response = await globalThis.fetch(
		new URL(`/api/v1/projects/${projectId}/search/stream`, globalThis.location?.origin),
		{ method: "POST", headers: { "Content-Type": "application/json" }, body, signal },
	);
	if (!response.ok || !response.body) {
		const text = await response.text();
		const type = response.headers.get("content-type") ?? "";
		throw new ApiError(
			response.status,
			text && type.includes("json") ? (JSON.parse(text) as Problem) : undefined,
		);
	}
	return lines(response.body);
}

/**
 * A project's search results for `query`, while it is not empty, shown
 * as the machine finds them. A new query, or leaving, cancels the search
 * under way, which stops it on the machine too.
 */
export function useSearch(projectId: string, query: SearchQuery) {
	const [state, dispatch] = useReducer(reduce, idle);
	const body = JSON.stringify(query);

	useEffect(() => {
		if (!query.query) {
			dispatch({ type: "idle" });
			return;
		}
		const abort = new AbortController();
		dispatch({ type: "start" });
		void (async () => {
			try {
				for await (const line of await stream(projectId, body, abort.signal)) {
					dispatch({ type: "frame", frame: JSON.parse(line) as SearchFrame });
				}
			} catch (error) {
				if (!abort.signal.aborted) dispatch({ type: "failed", error });
			}
		})();
		return () => abort.abort();
		// `body` stands for the query.
		// oxlint-disable-next-line react-hooks/exhaustive-deps
	}, [projectId, body]);

	return {
		data: state.data,
		error: state.error,
		isError: state.error !== undefined,
		isFetching: state.searching,
	};
}

/** A project's files, for quick open; asked when it opens. */
export function useFileIndex(projectId: string, enabled: boolean) {
	return useGeneratedFileIndex(projectId, {
		query: { enabled: enabled && Boolean(projectId), staleTime: 30_000, retry: false },
	});
}

/**
 * Where a whole word appears in a project, as places (the
 * approximate navigation while no language server runs).
 */
export async function findWord(
	projectId: string,
	word: string,
): Promise<{ path: string; line: number; character: number }[]> {
	const answer = await searchFiles(projectId, { query: word, wholeWord: true, matchCase: true });
	return answer.files.flatMap((file) =>
		file.matches.map((match) => ({
			path: file.path,
			line: match.line,
			character: match.ranges[0]?.[0] ?? 0,
		})),
	);
}
