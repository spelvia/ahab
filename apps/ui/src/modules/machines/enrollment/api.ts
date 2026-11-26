import {
	useApproveMachine as useGeneratedApproveMachine,
	useCreateJoinCode as useGeneratedCreateJoinCode,
	useDeleteJoinCode as useGeneratedDeleteJoinCode,
	useListJoinCodes,
} from "@/shared/api/generated/machines-enrollment/machines-enrollment";
import { useInvalidate } from "@/shared/api/invalidate";

/** Approves a waiting machine, then refreshes what shows machines. */
export function useApproveMachine() {
	return useGeneratedApproveMachine({ mutation: { onSuccess: useInvalidate("/machines") } });
}

/** Creates a one-time code for another computer to join. */
export function useCreateJoinCode() {
	return useGeneratedCreateJoinCode();
}

/** Withdraws a one-time code. */
export function useWithdrawJoinCode() {
	return useGeneratedDeleteJoinCode();
}

/**
 * The codes that still work, asked again every 3 seconds while the code
 * `watched` is among them: a code that leaves the list before it expires was
 * used.
 */
export function useJoinCodes(watched: string | undefined) {
	return useListJoinCodes({
		query: {
			enabled: watched !== undefined,
			refetchInterval: (query) =>
				query.state.data?.items.some((code) => code.id === watched) === false ? false : 3_000,
		},
	});
}
