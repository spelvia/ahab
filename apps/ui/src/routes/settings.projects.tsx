import { createFileRoute } from "@tanstack/react-router";

import { ProjectsPage } from "@/modules/projects";

export const Route = createFileRoute("/settings/projects")({
	component: ProjectsPage,
});
