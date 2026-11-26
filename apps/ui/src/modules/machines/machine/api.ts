import { useQueryClient } from "@tanstack/react-query";

import {
	getListMachinesQueryKey,
	useGetMachine,
	useListMachines,
	useRemoveMachine as useGeneratedRemoveMachine,
	useRenameMachine as useGeneratedRenameMachine,
} from "@/shared/api/generated/machines-machine/machines-machine";

/**
 * Every machine, asked again every 5 seconds so online state stays current
 * (until the live event stream exists).
 */
export function useMachines() {
	return useListMachines({ query: { refetchInterval: 5_000 } });
}

/** One machine, asked again every 3 seconds (a pending one may be approved). */
export function useMachine(id: string) {
	return useGetMachine(id, { query: { refetchInterval: 3_000 } });
}

/** Renames a machine, then refreshes the list. */
export function useRenameMachine() {
	const queryClient = useQueryClient();
	return useGeneratedRenameMachine({
		mutation: {
			onSuccess: () => queryClient.invalidateQueries({ queryKey: getListMachinesQueryKey() }),
		},
	});
}

/** Removes a machine, then refreshes the list. */
export function useRemoveMachine() {
	const queryClient = useQueryClient();
	return useGeneratedRemoveMachine({
		mutation: {
			onSuccess: () => queryClient.invalidateQueries({ queryKey: getListMachinesQueryKey() }),
		},
	});
}
