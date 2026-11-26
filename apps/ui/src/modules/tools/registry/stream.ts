import { useEffect, useRef } from "react";

import { useRefreshTools, useShowJob } from "@/modules/tools/registry/api";
import type { ToolJobEvent } from "@/modules/tools/registry/model";
import { keepOpen } from "@/shared/api/resume";

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
		if (!enabled || typeof EventSource === "undefined") return;
		const stream = keepOpen(() => {
			const source = new EventSource("/api/v1/plugin-events?names=tools.job");
			source.addEventListener("message", (message) => {
				const event = JSON.parse(String(message.data)) as { name: string; data: ToolJobEvent };
				if (event.name !== "tools.job" || event.data.machine !== machineId) return;
				const { machine: _machine, ...job } = event.data;
				latest.current.show(job);
				if (job.state !== "running") latest.current.refresh();
			});
			return source;
		});
		return () => stream.close();
	}, [machineId, enabled]);
}
