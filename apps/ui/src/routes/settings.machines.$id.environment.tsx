import { createFileRoute } from "@tanstack/react-router";

import { MachineEnvironment } from "@/modules/machines";

/** A machine's tools and sandboxes. */
export const Route = createFileRoute("/settings/machines/$id/environment")({
	component: MachineEnvironment,
});
