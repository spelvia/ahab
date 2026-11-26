import { createFileRoute, redirect } from "@tanstack/react-router";

/** The approval link `ahab setup` prints: machines now live in Settings. */
export const Route = createFileRoute("/machines/$id")({
	beforeLoad: ({ params }) => {
		throw redirect({ to: "/settings/machines/$id", params: { id: params.id }, replace: true });
	},
});
