import { useCallback, useEffect, useRef, useState } from "react";

import { useRequestPairing, waitForAnswer } from "@/modules/clients/pairing/api";
import { problemCode } from "@/shared/api/fetch";
import type { CreatedPairingRequest } from "@/shared/api/generated/ahab.schemas";

const KEY = "ahab.pairing-request";

/** Where this browser's request to be paired stands. */
export type RequestStage = "asking" | "waiting" | "denied" | "failed";

function stored(now = new Date()): CreatedPairingRequest | undefined {
	try {
		const text = sessionStorage.getItem(KEY);
		if (!text) return undefined;
		const request = JSON.parse(text) as CreatedPairingRequest;
		return new Date(request.expiresAt) > now ? request : undefined;
	} catch {
		return undefined;
	}
}

const pause = (ms: number, signal: AbortSignal) =>
	new Promise<void>((resolve) => {
		const timer = setTimeout(resolve, ms);
		signal.addEventListener("abort", () => {
			clearTimeout(timer);
			resolve();
		});
	});

/**
 * This browser asks to be paired and waits for the answer: the code to
 * show, and `onPaired` once a paired client approves. The tab keeps its
 * request, so a refresh shows the same code.
 */
export function usePairingRequest(onPaired: () => void) {
	const ask = useRequestPairing();
	const [request, setRequest] = useState(() => stored());
	const [stage, setStage] = useState<RequestStage>(request ? "waiting" : "asking");
	const [error, setError] = useState<string>();
	const paired = useRef(onPaired);
	useEffect(() => {
		paired.current = onPaired;
	});
	const asking = useRef(false);

	const askAgain = useCallback(() => {
		sessionStorage.removeItem(KEY);
		setRequest(undefined);
		setStage("asking");
	}, []);

	// Ask, unless the tab keeps a request still open.
	useEffect(() => {
		if (stage !== "asking" || asking.current) return;
		asking.current = true;
		ask.mutate(
			{ data: { kind: "browser" } },
			{
				onSuccess: (made) => {
					sessionStorage.setItem(KEY, JSON.stringify(made));
					setRequest(made);
					setStage("waiting");
				},
				onError: (failed) => {
					setError(failed.message);
					setStage("failed");
				},
				onSettled: () => {
					asking.current = false;
				},
			},
		);
	}, [stage, ask]);

	// Wait for the answer, one long poll after another.
	useEffect(() => {
		if (stage !== "waiting" || !request) return;
		const stop = new AbortController();
		void (async () => {
			while (!stop.signal.aborted) {
				try {
					// One poll after another, on purpose.
					// oxlint-disable-next-line no-await-in-loop
					const answer = await waitForAnswer(request.id, request.secret, stop.signal);
					if (stop.signal.aborted) return;
					if (answer.state === "used") {
						sessionStorage.removeItem(KEY);
						paired.current();
						return;
					}
					if (answer.state === "denied") {
						sessionStorage.removeItem(KEY);
						setStage("denied");
						return;
					}
				} catch (failed) {
					if (stop.signal.aborted) return;
					const code = problemCode(failed);
					if (code === "pairing_expired" || code === "pairing_invalid") {
						// Ran out of time: ask again, with a new code.
						askAgain();
						return;
					}
					// oxlint-disable-next-line no-await-in-loop
					await pause(code === "too_many_attempts" ? 60_000 : 3_000, stop.signal);
				}
			}
		})();
		return () => stop.abort();
	}, [stage, request, askAgain]);

	return { request, stage, error, askAgain };
}
