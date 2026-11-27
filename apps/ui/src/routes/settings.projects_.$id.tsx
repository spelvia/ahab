import { createFileRoute, Outlet } from "@tanstack/react-router";

import { ProjectLayout } from "@/modules/projects";

/** One project: its header and tabs, the open tab below. */
export const Route = createFileRoute("/settings/projects_/$id")({
	component: ProjectRoute,
});

function ProjectRoute() {
	const { id } = Route.useParams();
	return (
		<ProjectLayout key={id} projectId={id}>
			<Outlet />
		</ProjectLayout>
	);
}
