import { createFileRoute } from "@tanstack/react-router";

import { MachineResources } from "@/modules/machines";

/** A machine's CPU, memory, disks and processes, live. */
export const Route = createFileRoute("/settings/machines/$id/resources")({
	component: ResourcesRoute,
});

function ResourcesRoute() {
	const { id } = Route.useParams();
	return <MachineResources machineId={id} />;
}
