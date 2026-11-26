import { createFileRoute } from "@tanstack/react-router";

import { NotificationsPage } from "@/modules/notifications";

export const Route = createFileRoute("/settings/notifications")({
	component: NotificationsPage,
});
