import { createFileRoute } from "@tanstack/react-router";

import { EditorSettingsPage } from "@/modules/platform";

export const Route = createFileRoute("/settings/editor")({
	component: EditorSettingsPage,
});
