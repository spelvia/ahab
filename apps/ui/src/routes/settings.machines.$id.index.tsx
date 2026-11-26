import { createFileRoute } from "@tanstack/react-router";

import { MachineOverview } from "@/modules/machines";
import { MachinePluginGlance } from "@/modules/plugins";

/** A machine's overview, with what the plugins module knows about it. */
export const Route = createFileRoute("/settings/machines/$id/")({
	component: OverviewRoute,
});

function OverviewRoute() {
	const { id } = Route.useParams();
	return <MachineOverview id={id} glance={<MachinePluginGlance machineId={id} />} />;
}
