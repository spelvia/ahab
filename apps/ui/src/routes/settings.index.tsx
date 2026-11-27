import { createFileRoute } from "@tanstack/react-router";

import { SettingsOverview } from "@/app/shell/settings-overview";

/** Settings opens on its overview. */
export const Route = createFileRoute("/settings/")({
	component: SettingsOverview,
});
