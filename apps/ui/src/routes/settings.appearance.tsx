import { createFileRoute } from "@tanstack/react-router";

import { AppearancePage } from "@/modules/platform";

export const Route = createFileRoute("/settings/appearance")({
	component: AppearancePage,
});
