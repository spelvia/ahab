import { createFileRoute } from "@tanstack/react-router";

import { ClientPage } from "@/modules/clients";

/** A paired client's page: how and when it was paired and used; rename, revoke. */
export const Route = createFileRoute("/settings/clients_/$id")({
	component: function ClientRoute() {
		const { id } = Route.useParams();
		return <ClientPage key={id} clientId={id} />;
	},
});
