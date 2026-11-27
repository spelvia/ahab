import { createFileRoute, useParams } from "@tanstack/react-router";

import { WorkbenchPage } from "@/app/shell/workbench";

/**
 * A project: the workbench stays while its child routes only say which
 * session's window is in front.
 */
export const Route = createFileRoute("/p/$project")({
	component: Page,
});

function Page() {
	const { project } = Route.useParams();
	const { session } = useParams({ strict: false });
	return <WorkbenchPage key={project} project={project} session={session} />;
}
