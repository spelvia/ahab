import { useEffect, useRef } from "react";

import { useRefreshTools, useShowJob } from "@/modules/tools/registry/api";
import type { ToolJobEvent } from "@/modules/tools/registry/model";
import { onEvent } from "@/shared/api/events";

/**
 * Follows a machine's installs, updates and removals on the server's event
 * stream (`tools.job`): each step joins the machine's list as it comes, and
 * the list is asked again when a job ends.
 */
export function useToolJobs(machineId: string, enabled = true) {
	const show = useShowJob(machineId);
	const refresh = useRefreshTools(machineId);
	const latest = useRef({ show, refresh });
	useEffect(() => {
		latest.current = { show, refresh };
	});
	useEffect(() => {
		if (!enabled) return;
		// On the app's one shared event stream (see `@/shared/api/events`).
		return onEvent("tools.job", (data) => {
			const event = data as ToolJobEvent;
			if (event.machine !== machineId) return;
			const { machine: _machine, ...job } = event;
			latest.current.show(job);
			if (job.state !== "running") latest.current.refresh();
		});
	}, [machineId, enabled]);
}
