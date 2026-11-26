import { createFileRoute } from "@tanstack/react-router";

import { PairPage } from "@/modules/clients";

export const Route = createFileRoute("/pair")({
	validateSearch: (search: Record<string, unknown>): { next?: string } =>
		typeof search.next === "string" ? { next: search.next } : {},
	component: function Pair() {
		const { next } = Route.useSearch();
		return <PairPage next={next} />;
	},
});
