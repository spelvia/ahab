import { createRouter, ErrorComponent } from "@tanstack/react-router";

import { queryClient } from "@/app/query-client";
import { routeTree } from "@/routeTree.gen";
import { SplashDone } from "@/shared/splash/splash-done";

export const router = createRouter({
	routeTree,
	context: { queryClient },
	defaultPreload: "intent",
	scrollRestoration: true,
	// An error takes over from the start-up splash too.
	defaultErrorComponent: (props) => (
		<>
			<SplashDone />
			<ErrorComponent {...props} />
		</>
	),
});

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}
