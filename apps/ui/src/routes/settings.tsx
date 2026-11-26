import { createFileRoute } from "@tanstack/react-router";

import { SettingsLayout } from "@/app/shell/settings-layout";

export const Route = createFileRoute("/settings")({
	component: SettingsLayout,
});
