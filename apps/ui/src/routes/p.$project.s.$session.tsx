import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";

import { showPart } from "@/modules/sessions";

/**
 * A session's window in front; the project's route draws the workbench.
 * `?chat=` (a notification's) brings that chat's tab forward.
 */
export const Route = createFileRoute("/p/$project/s/$session")({
	validateSearch: (search: Record<string, unknown>): { chat?: string } =>
		typeof search.chat === "string" ? { chat: search.chat } : {},
	component: SessionRoute,
});

function SessionRoute() {
	const { session } = Route.useParams();
	const { chat } = Route.useSearch();
	useEffect(() => {
		if (chat) showPart(session, `chat:${chat}`);
	}, [session, chat]);
	return null;
}
