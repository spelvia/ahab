import { useQueryClient } from "@tanstack/react-query";

import type { ToolSettings } from "@/shared/api/generated/ahab.schemas";
import {
	getGetMachineToolSettingsQueryKey,
	getGetProjectToolSettingsQueryKey,
	useGetMachineToolSettings,
	useGetProjectToolSettings,
	usePutMachineToolSettings,
	usePutProjectToolSettings,
} from "@/shared/api/generated/tools-setting/tools-setting";
import { useInvalidate } from "@/shared/api/invalidate";

/** A machine's tool settings: paths, environment, tools off, your own tools, limits. */
export function useMachineToolSettings(machineId: string) {
	return useGetMachineToolSettings(machineId, { query: { retry: false } });
}

/**
 * Saves a machine's tool settings whole, shows them at once, and asks for
 * the machine's tools again, whose states may follow from them.
 */
export function useSaveMachineToolSettings(machineId: string) {
	const queryClient = useQueryClient();
	const refreshTools = useInvalidate(`/machines/${machineId}/tools`);
	const save = usePutMachineToolSettings({
		mutation: {
			onSuccess: (settings) => {
				queryClient.setQueryData(getGetMachineToolSettingsQueryKey(machineId), settings);
				refreshTools();
			},
		},
	});
	return {
		...save,
		save: (data: ToolSettings, options?: { onSuccess?: () => void }) =>
			save.mutate({ id: machineId, data }, options),
	};
}

/** A project's tool settings, which follow it to every machine. */
export function useProjectToolSettings(projectId: string) {
	return useGetProjectToolSettings(projectId, { query: { retry: false } });
}

/** Saves a project's tool settings whole and shows them at once. */
export function useSaveProjectToolSettings(projectId: string) {
	const queryClient = useQueryClient();
	const save = usePutProjectToolSettings({
		mutation: {
			onSuccess: (settings) =>
				queryClient.setQueryData(getGetProjectToolSettingsQueryKey(projectId), settings),
		},
	});
	return {
		...save,
		save: (data: ToolSettings, options?: { onSuccess?: () => void }) =>
			save.mutate({ id: projectId, data }, options),
	};
}
