import { useMachineMetrics as useGeneratedMachineMetrics } from "@/shared/api/generated/machines-metrics/machines-metrics";

/** A machine's resources, sampled again every 3 seconds while shown. */
export function useMachineMetrics(machineId: string) {
	return useGeneratedMachineMetrics(machineId, {
		query: { refetchInterval: 3_000, retry: false, refetchIntervalInBackground: false },
	});
}

/** A machine's resources, sampled now and then, for its overview. */
export function useMachineMetricsNowAndThen(machineId: string, enabled: boolean) {
	return useGeneratedMachineMetrics(machineId, {
		query: { enabled, refetchInterval: 30_000, retry: false },
	});
}
