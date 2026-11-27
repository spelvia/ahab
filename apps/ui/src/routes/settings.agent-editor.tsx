import { createFileRoute } from "@tanstack/react-router";

import { AgentEditorPage } from "@/modules/platform";

export const Route = createFileRoute("/settings/agent-editor")({
	component: AgentEditorPage,
});
