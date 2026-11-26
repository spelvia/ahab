import { createFileRoute } from "@tanstack/react-router";

import { StartPage } from "@/modules/projects";

/** No home page: back to the last project, or first run. */
export const Route = createFileRoute("/")({
	component: StartPage,
});
