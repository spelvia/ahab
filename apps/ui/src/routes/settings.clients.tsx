import { createFileRoute } from "@tanstack/react-router";

import { ClientsPage } from "@/modules/clients";

export const Route = createFileRoute("/settings/clients")({
	validateSearch: (search: Record<string, unknown>): { approve?: string } =>
		typeof search.approve === "string" ? { approve: search.approve } : {},
	component: function Clients() {
		const { approve } = Route.useSearch();
		return <ClientsPage approve={approve} />;
	},
});
