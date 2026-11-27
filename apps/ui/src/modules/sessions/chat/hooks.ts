import {
	type RefObject,
	useCallback,
	useEffect,
	useEffectEvent,
	useMemo,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";

import { useStopWork } from "@/modules/sessions/chat/api";
import { active, type StopState, stopOutcome } from "@/modules/sessions/chat/model";
import { t } from "@/shared/i18n";
import { toast } from "@/shared/ui/toast";

/**
 * A message to show once its chat is open: a search found it. Only the
 * last asked for is kept; the conversation that holds it takes it.
 */
let wanted: { chatId: string; seq: number } | null = null;
const listeners = new Set<() => void>();

/** Asks the chat's conversation to scroll to its message `seq`, when shown. */
export function revealMessage(chatId: string, seq: number) {
	wanted = { chatId, seq };
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

export function useWantedMessage(chatId: string | undefined) {
	const asked = useSyncExternalStore(subscribe, () => wanted);
	return asked?.chatId === chatId ? asked : null;
}

/**
 * Scrolls `list` to the message a search asked for in `chatId`, and marks
 * it for a moment, once its row (`data-seq`) is there; `version` changes
 * as rows arrive.
 */
export function useRevealedMessage(
	chatId: string | undefined,
	list: RefObject<HTMLElement | null>,
	version: unknown,
) {
	const asked = useWantedMessage(chatId);
	useEffect(() => {
		if (!chatId || !asked || asked.chatId !== chatId) return;
		const row = list.current?.querySelector<HTMLElement>(`[data-seq="${asked.seq}"]`);
		if (!row) return;
		wanted = null;
		for (const listener of listeners) listener();
		row.scrollIntoView({ block: "center" });
		row.dataset.revealed = "";
		setTimeout(() => delete row.dataset.revealed, 2_000);
		// `version` is what changes as rows arrive: then the row is looked for again.
		// oxlint-disable-next-line react/exhaustive-effect-dependencies
	}, [chatId, asked, list, version]);
}

/** How long a stop the agent accepted waits for it to report the work stopped. */
export const STOP_WAIT_MS = 20_000;

/**
 * Stopping a chat's subagents and background tasks, each on its own: a stop
 * the agent accepts shows "Stopping…" until the agent reports the work no
 * longer runs (`statusOf`), or a while passes; a refusal shows as an error
 * in its row; what is settled says so in a toast.
 */
export function useSideWorkStops(chatId: string, statusOf: (id: string) => string | undefined) {
	const stopWork = useStopWork().mutateAsync;
	const [stops, setStops] = useState<Readonly<Record<string, StopState>>>({});
	const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
	const names = useRef(new Map<string, string>());
	const put = useCallback((id: string, state: StopState | undefined) => {
		const timer = timers.current.get(id);
		if (timer) clearTimeout(timer);
		timers.current.delete(id);
		if (state?.phase === "stopping")
			timers.current.set(
				id,
				setTimeout(() => {
					timers.current.delete(id);
					setStops((all) =>
						all[id]?.phase === "stopping" ? { ...all, [id]: { phase: "timedOut" } } : all,
					);
				}, STOP_WAIT_MS),
			);
		setStops((all) => {
			if (!state) {
				if (!(id in all)) return all;
				const { [id]: _gone, ...rest } = all;
				return rest;
			}
			return { ...all, [id]: state };
		});
	}, []);
	useEffect(() => {
		const pending = timers.current;
		return () => {
			for (const timer of pending.values()) clearTimeout(timer);
		};
	}, []);
	// The agent reports it no longer runs: stopped (or finished meanwhile).
	// Its stop is over; it is said once, and its row shows no stop state.
	const settled = new Set(
		Object.keys(stops).filter((id) => {
			const status = statusOf(id);
			return status !== undefined && !active(status);
		}),
	);
	const settledKey = [...settled].join("\n");
	const said = useRef(new Set<string>());
	const announce = useEffectEvent((key: string) => {
		for (const id of key.split("\n")) {
			if (said.current.has(id)) continue;
			said.current.add(id);
			const timer = timers.current.get(id);
			if (timer) clearTimeout(timer);
			timers.current.delete(id);
			if (stops[id]?.phase === "stopping")
				toast(t("sessions:activity.stop.stopped", { name: names.current.get(id) ?? "" }));
		}
	});
	useEffect(() => {
		if (settledKey) announce(settledKey);
	}, [settledKey]);
	const shown = useMemo(() => {
		if (!settledKey) return stops;
		const gone = new Set(settledKey.split("\n"));
		return Object.fromEntries(Object.entries(stops).filter(([id]) => !gone.has(id)));
	}, [stops, settledKey]);
	const stop = useCallback(
		(type: "agent" | "task", id: string, name: string) => {
			names.current.set(id, name);
			said.current.delete(id);
			put(id, { phase: "stopping" });
			stopWork({ id: chatId, data: { type, id } }).then(
				(receipt) => {
					const outcome = stopOutcome(receipt.disposition);
					if (outcome === "rejected")
						put(id, { phase: "failed", message: receipt.message ?? undefined });
					else if (outcome === "stopping")
						put(id, { phase: "stopping", unconfirmed: receipt.disposition === "unconfirmed" });
					else {
						put(id, undefined);
						toast(
							t(
								outcome === "stopped"
									? "sessions:activity.stop.stopped"
									: "sessions:activity.stop.finished",
								{ name },
							),
						);
					}
				},
				(error: Error) => put(id, { phase: "failed", message: error.message }),
			);
		},
		[chatId, put, stopWork],
	);
	return { stops: shown, stop };
}
