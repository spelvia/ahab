import { createFileRoute } from "@tanstack/react-router";

import { MachinesPage } from "@/modules/machines";

export const Route = createFileRoute("/settings/machines/")({
	component: MachinesPage,
});
