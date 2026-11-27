import { createFileRoute } from "@tanstack/react-router";

import { SettingsAbout } from "@/app/shell/settings-about";

export const Route = createFileRoute("/settings/about")({
	component: SettingsAbout,
});
