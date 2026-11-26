import { createFileRoute } from "@tanstack/react-router";

import { KeyboardPage } from "@/modules/platform";

export const Route = createFileRoute("/settings/keyboard")({
	component: KeyboardPage,
});
