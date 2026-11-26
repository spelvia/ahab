import { createFileRoute } from "@tanstack/react-router";

import { MachinePlugins } from "@/modules/plugins";

/** The plugins running a part on a machine. */
export const Route = createFileRoute("/settings/machines/$id/plugins")({
	component: PluginsRoute,
});

function PluginsRoute() {
	const { id } = Route.useParams();
	return <MachinePlugins machineId={id} />;
}
