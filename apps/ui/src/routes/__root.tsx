import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext } from "@tanstack/react-router";

import { NotFound, RootLayout } from "@/app/shell/root-layout";

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
	component: RootLayout,
	notFoundComponent: NotFound,
});
