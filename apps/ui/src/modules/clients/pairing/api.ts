import {
	useApprovePairingRequest as useGeneratedApprove,
	useCreatePairing as useGeneratedCreatePairing,
	useDenyPairingRequest as useGeneratedDeny,
	useGetPairing,
	useListPairingRequests,
	usePair as useGeneratedPair,
	useRequestPairing as useGeneratedRequestPairing,
	useWithdrawPairing as useGeneratedWithdraw,
	waitForAnswer as generatedWaitForAnswer,
} from "@/shared/api/generated/clients-pairing/clients-pairing";
import { useInvalidate } from "@/shared/api/invalidate";

/** Creates a pairing link and code for a new client (Add client). */
export function useCreatePairing() {
	return useGeneratedCreatePairing();
}

/** A link this browser offered, asked again every 2 seconds while open. */
export function usePairingLink(id: string | undefined) {
	return useGetPairing(id ?? "", {
		query: {
			enabled: id !== undefined,
			refetchInterval: (query) => (query.state.data?.state === "open" ? 2_000 : false),
		},
	});
}

/** Withdraws a link before it is used. */
export function useWithdrawPairing() {
	return useGeneratedWithdraw();
}

/** Pairs this browser with a link's secret or a code. */
export function usePair() {
	return useGeneratedPair();
}

/** This browser asks to be paired. */
export function useRequestPairing() {
	return useGeneratedRequestPairing();
}

/** Waits (up to 25 seconds) for the answer to this browser's request. */
export function waitForAnswer(id: string, secret: string, signal?: AbortSignal) {
	return generatedWaitForAnswer(id, { secret, wait: 25 }, { signal });
}

/** The requests waiting for an answer. */
export function usePairingRequests(enabled = true) {
	return useListPairingRequests({ query: { enabled, refetchInterval: 30_000 } });
}

/** Approves a request, then refreshes the requests and clients. */
export function useApprovePairingRequest() {
	return useGeneratedApprove({
		mutation: { onSuccess: useInvalidate("/pairing-requests", "/clients") },
	});
}

/** Denies a request, then refreshes the requests. */
export function useDenyPairingRequest() {
	return useGeneratedDeny({ mutation: { onSuccess: useInvalidate("/pairing-requests") } });
}
