import { createFileRoute } from "@tanstack/react-router";

import { MachineSettings } from "@/modules/machines";

/** What is set for a machine alone. */
export const Route = createFileRoute("/settings/machines/$id/settings")({
	component: MachineSettings,
});
