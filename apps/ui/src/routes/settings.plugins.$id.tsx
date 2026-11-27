import { createFileRoute } from "@tanstack/react-router";

import { PluginPage } from "@/modules/plugins";

interface Search {
	enable?: boolean;
	warnings?: string;
}

export const Route = createFileRoute("/settings/plugins/$id")({
	validateSearch: (search: Record<string, unknown>): Search => ({
		enable: search.enable === true || search.enable === "true" ? true : undefined,
		warnings: typeof search.warnings === "string" ? search.warnings : undefined,
	}),
	component: PluginRoute,
});

function PluginRoute() {
	const { id } = Route.useParams();
	const { enable, warnings } = Route.useSearch();
	return <PluginPage key={id} id={id} enable={enable} warnings={warnings} />;
}
