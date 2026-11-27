import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
	agentUsage,
	getAgentUsageQueryKey,
	useAgentUsage,
} from "@/shared/api/generated/machines-usage/machines-usage";

/**
 * The plan limits of the agents signed in on a machine, kept fresh while
 * shown (the server asks the machine at most every 5 minutes).
 */
export function useAgentUsageOn(machineId: string | undefined, enabled = true) {
	return useAgentUsage(machineId ?? "", undefined, {
		query: {
			enabled: enabled && Boolean(machineId),
			staleTime: 60_000,
			refetchInterval: 5 * 60_000,
			retry: false,
		},
	});
}

/** Asks the machine for its agents' plan limits now. */
export function useRefreshAgentUsage(machineId: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: () => agentUsage(machineId, { refresh: true }),
		onSuccess: (list) => queryClient.setQueryData(getAgentUsageQueryKey(machineId), list),
	});
}
