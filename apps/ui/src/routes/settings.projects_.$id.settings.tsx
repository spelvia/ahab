import { createFileRoute } from "@tanstack/react-router";

import { ProjectPage } from "@/modules/projects";

/** A project's settings: its name, icon and group, its folder, editor, archive and delete. */
export const Route = createFileRoute("/settings/projects_/$id/settings")({
	component: ProjectSettingsRoute,
});

function ProjectSettingsRoute() {
	const { id } = Route.useParams();
	return <ProjectPage projectId={id} />;
}
