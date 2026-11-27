import { createFileRoute } from "@tanstack/react-router";

import { ProjectLspsPage } from "@/modules/tools";

/** A project's language tool choices, beside Settings → Projects. */
export const Route = createFileRoute("/settings/projects_/$id/lsps")({
	component: ProjectLspsRoute,
});

function ProjectLspsRoute() {
	const { id } = Route.useParams();
	return <ProjectLspsPage projectId={id} />;
}
