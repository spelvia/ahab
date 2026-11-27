import { createFileRoute } from "@tanstack/react-router";

import { ProjectOverview } from "@/modules/projects";
import { ProjectSessions } from "@/modules/sessions";
import type { Project } from "@/shared/api/generated/ahab.schemas";

/** A project's Overview: the project at a glance, with what runs in it. */
export const Route = createFileRoute("/settings/projects_/$id/")({
	component: OverviewRoute,
});

/** What runs in the project, on its card. */
function sessionsOf(project: Project) {
	return <ProjectSessions project={project} />;
}

function OverviewRoute() {
	const { id } = Route.useParams();
	return <ProjectOverview projectId={id} sessions={sessionsOf} />;
}
