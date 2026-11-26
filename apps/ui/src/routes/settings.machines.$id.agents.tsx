import { createFileRoute } from "@tanstack/react-router";

import { MachineAgentsPage } from "@/modules/machines";
import { AgentModels } from "@/modules/sessions";

interface Search {
	/** The agent whose details are open. */
	agent?: string;
}

/** A machine's agents, with the models each offers there; `?agent=` opens one. */
export const Route = createFileRoute("/settings/machines/$id/agents")({
	validateSearch: (search: Record<string, unknown>): Search => ({
		agent: typeof search.agent === "string" ? search.agent : undefined,
	}),
	component: AgentsRoute,
});

function AgentsRoute() {
	const { id } = Route.useParams();
	const { agent } = Route.useSearch();
	const navigate = Route.useNavigate();
	return (
		<MachineAgentsPage
			id={id}
			Models={AgentModels}
			selected={agent}
			onSelect={(next) => void navigate({ search: { agent: next }, replace: true })}
		/>
	);
}
