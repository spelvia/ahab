import { createFileRoute } from "@tanstack/react-router";

import { PluginsPage } from "@/modules/plugins";

export const Route = createFileRoute("/settings/plugins/")({
	component: PluginsPage,
});
