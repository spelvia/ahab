import { createFileRoute } from "@tanstack/react-router";

/** The project itself: its windows, no session in front. */
export const Route = createFileRoute("/p/$project/")({
	component: () => null,
});
