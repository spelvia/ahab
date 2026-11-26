import { useEffect, useRef } from "react";

import {
	useRefreshActivity,
	useRefreshProblems,
	useRefreshTurn,
} from "@/modules/tools/problems/api";

/** A project's problems changed (`problems.changed`). */
interface ProblemsChanged {
	project: string;
	source: string;
}

/** A turn's difference is ready (`problems.turn`). */
interface ProblemsTurn {
	chat: string;
	turn: string;
	project: string;
}

/** A repository was used or pinned (`repositories.activity`). */
interface ActivityChanged {
	project: string;
}

type ProblemEvent =
	| { name: "problems.changed"; data: ProblemsChanged }
	| { name: "problems.turn"; data: ProblemsTurn }
	| { name: "repositories.activity"; data: ActivityChanged };

type Listener = (event: ProblemEvent) => void;

// One connection for every view listening: each finished turn in a chat
// listens, and a page may show many.
const listeners = new Set<Listener>();
let source: EventSource | undefined;

function listen(listener: Listener): () => void {
	listeners.add(listener);
	if (!source && typeof EventSource !== "undefined") {
		source = new EventSource(
			"/api/v1/plugin-events?names=problems.changed,problems.turn,repositories.activity",
		);
		source.addEventListener("message", (message) => {
			const event = JSON.parse(String(message.data)) as ProblemEvent;
			for (const each of listeners) each(event);
		});
	}
	return () => {
		listeners.delete(listener);
		if (listeners.size === 0) {
			source?.close();
			source = undefined;
		}
	};
}

/** Calls `changed` whenever a project's problems change; answers what stops it. */
export function onProblemsChanged(projectId: string, changed: () => void): () => void {
	return listen((event) => {
		if (event.name === "problems.changed" && event.data.project === projectId) changed();
	});
}

/** Keeps a project's problems (and its totals) current as they change. */
export function useProblemEvents(projectId: string | undefined) {
	const refresh = useRefreshProblems();
	const latest = useRef(refresh);
	useEffect(() => {
		latest.current = refresh;
	});
	useEffect(() => {
		if (!projectId) return;
		return listen((event) => {
			if (event.name === "problems.changed" && event.data.project === projectId)
				latest.current(projectId);
		});
	}, [projectId]);
}

/** Keeps a project's repositories' use current as files open and pins change. */
export function useActivityEvents(projectId: string | undefined) {
	const refresh = useRefreshActivity();
	const latest = useRef(refresh);
	useEffect(() => {
		latest.current = refresh;
	});
	useEffect(() => {
		if (!projectId) return;
		return listen((event) => {
			if (event.name === "repositories.activity" && event.data.project === projectId)
				latest.current(projectId);
		});
	}, [projectId]);
}

/** Asks for a turn's difference again once the server has compared it. */
export function useTurnEvents(chatId: string, turnId: string) {
	const refresh = useRefreshTurn();
	const latest = useRef(refresh);
	useEffect(() => {
		latest.current = refresh;
	});
	useEffect(
		() =>
			listen((event) => {
				if (
					event.name === "problems.turn" &&
					event.data.chat === chatId &&
					event.data.turn === turnId
				)
					latest.current(chatId, turnId);
			}),
		[chatId, turnId],
	);
}
