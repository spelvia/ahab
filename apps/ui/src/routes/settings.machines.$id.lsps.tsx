import { createFileRoute } from "@tanstack/react-router";

import { MachineLspsPage } from "@/modules/tools";

/** A machine's language tools and running language servers. */
export const Route = createFileRoute("/settings/machines/$id/lsps")({
	component: LspsRoute,
});

function LspsRoute() {
	const { id } = Route.useParams();
	return <MachineLspsPage id={id} />;
}
