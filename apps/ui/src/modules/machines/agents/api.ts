import { useQueryClient } from "@tanstack/react-query";

import {
	useAddAgent as useGeneratedAddAgent,
	useAgentCatalog as useGeneratedAgentCatalog,
	useRemoveAgent as useGeneratedRemoveAgent,
	agentUpdates,
	getAgentUpdatesQueryKey,
	getListAgentsQueryKey,
	useAgentUpdates as useGeneratedAgentUpdates,
	useUpdateAgent as useGeneratedUpdateAgent,
	useListAgents,
	useRefreshAgents as useGeneratedRefreshAgents,
} from "@/shared/api/generated/machines-agents/machines-agents";

/** The agents on a machine, as last detected. */
export function useAgents(machineId: string) {
	return useListAgents(machineId, { query: { refetchInterval: 30_000 } });
}

/** Detects a machine's agents again, now, and shows the answer at once. */
export function useRefreshAgents(machineId: string) {
	const queryClient = useQueryClient();
	return useGeneratedRefreshAgents({
		mutation: {
			onSuccess: (list) => queryClient.setQueryData(getListAgentsQueryKey(machineId), list),
		},
	});
}

/** Whether newer versions of a machine's agents are out (checked hourly by the server). */
export function useAgentUpdates(machineId: string, enabled = true) {
	return useGeneratedAgentUpdates(machineId, undefined, {
		query: { enabled, staleTime: 10 * 60_000, retry: false },
	});
}

/** Checks a machine's agents for updates now. */
export function useCheckAgentUpdates(machineId: string) {
	const queryClient = useQueryClient();
	return {
		check: () =>
			queryClient.fetchQuery({
				queryKey: getAgentUpdatesQueryKey(machineId),
				queryFn: () => agentUpdates(machineId, { refresh: true }),
				staleTime: 0,
			}),
	};
}

/** Updates an agent on a machine, then shows its new state and version. */
export function useUpdateAgent(machineId: string) {
	const queryClient = useQueryClient();
	return useGeneratedUpdateAgent({
		mutation: {
			onSettled: () =>
				Promise.all([
					queryClient.invalidateQueries({ queryKey: getListAgentsQueryKey(machineId) }),
					queryClient.invalidateQueries({ queryKey: getAgentUpdatesQueryKey(machineId) }),
				]),
		},
	});
}

/** Every agent Ahab can run: its own and the ACP registry's (refreshed daily by the server). */
export function useAgentCatalog() {
	return useGeneratedAgentCatalog({ query: { staleTime: 60 * 60_000 } });
}

/**
 * Detection after an agent was added or removed runs on the machine; its
 * list is asked again a few times while it does.
 */
function useFollowDetection(machineId: string) {
	const queryClient = useQueryClient();
	return () => {
		for (const delay of [500, 3_000, 10_000, 30_000, 70_000])
			setTimeout(
				() => void queryClient.invalidateQueries({ queryKey: getListAgentsQueryKey(machineId) }),
				delay,
			);
	};
}

/** Adds a registry or custom agent to a machine. */
export function useAddAgent(machineId: string) {
	const follow = useFollowDetection(machineId);
	return useGeneratedAddAgent({ mutation: { onSuccess: follow } });
}

/** Removes an added agent from a machine. */
export function useRemoveAgent(machineId: string) {
	const follow = useFollowDetection(machineId);
	return useGeneratedRemoveAgent({ mutation: { onSuccess: follow } });
}
