import { createFileRoute, Outlet } from "@tanstack/react-router";

import { MachineLayout } from "@/modules/machines";

/** One machine: its header and tabs, the open tab below. */
export const Route = createFileRoute("/settings/machines/$id")({
	component: MachineRoute,
});

function MachineRoute() {
	const { id } = Route.useParams();
	return (
		<MachineLayout key={id} id={id}>
			<Outlet />
		</MachineLayout>
	);
}
