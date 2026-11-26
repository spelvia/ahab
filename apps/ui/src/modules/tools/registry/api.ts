import { useMutation, useQueryClient } from "@tanstack/react-query";

import { withJob } from "@/modules/tools/registry/model";
import type { MachineToolsDto, ToolJobDto } from "@/shared/api/generated/ahab.schemas";
import {
	getListToolsQueryKey,
	listTools,
	useInstallTool as useGeneratedInstallTool,
	useListTools,
	useRemoveTool as useGeneratedRemoveTool,
	useUpdateTool as useGeneratedUpdateTool,
} from "@/shared/api/generated/tools-registry/tools-registry";

/** Every tool record with its state on a machine, and the machine's runtimes. */
export function useMachineTools(machineId: string, enabled = true) {
	return useListTools(machineId, undefined, {
		query: { enabled, retry: false, staleTime: 30_000 },
	});
}

/** Asks the machine to look for its tools and runtimes again, and shows the answer. */
export function useRescanTools(machineId: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: () => listTools(machineId, { rescan: true }),
		onSuccess: (list) => queryClient.setQueryData(getListToolsQueryKey(machineId), list),
	});
}

/** Puts a job into the machine's list at once, ahead of its events. */
export function useShowJob(machineId: string) {
	const queryClient = useQueryClient();
	return (job: ToolJobDto) =>
		queryClient.setQueryData<MachineToolsDto>(getListToolsQueryKey(machineId), (list) =>
			list ? withJob(list, job) : list,
		);
}

/** Installs a tool on a machine; its progress follows on the event stream. */
export function useInstallTool(machineId: string) {
	const show = useShowJob(machineId);
	return useGeneratedInstallTool({ mutation: { onSuccess: show } });
}

/** Installs a tool's newer version on a machine. */
export function useUpdateTool(machineId: string) {
	const show = useShowJob(machineId);
	return useGeneratedUpdateTool({ mutation: { onSuccess: show } });
}

/** Removes a tool Ahab installed on a machine. */
export function useRemoveTool(machineId: string) {
	const show = useShowJob(machineId);
	return useGeneratedRemoveTool({ mutation: { onSuccess: show } });
}

/** Asks for a machine's tools again, after a job ends or settings change. */
export function useRefreshTools(machineId: string) {
	const queryClient = useQueryClient();
	return () => void queryClient.invalidateQueries({ queryKey: getListToolsQueryKey(machineId) });
}
