import { createFileRoute } from "@tanstack/react-router";

import { FeaturesPage } from "@/modules/platform";

export const Route = createFileRoute("/settings/features")({
	component: FeaturesPage,
});
