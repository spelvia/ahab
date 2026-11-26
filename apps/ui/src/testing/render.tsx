import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";

function testQueryClient() {
	return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

/** Renders a component with a fresh query client that does not retry. */
export function renderWithQuery(ui: ReactElement) {
	return render(<QueryClientProvider client={testQueryClient()}>{ui}</QueryClientProvider>);
}

/**
 * Renders a component inside a router as well, for components with links,
 * at `path`. The router renders asynchronously: use `findBy…` queries.
 */
export function renderWithRouter(ui: ReactElement, path = "/") {
	const router = createRouter({
		routeTree: createRootRoute({ component: () => ui }),
		history: createMemoryHistory({ initialEntries: [path] }),
	});
	const queryClient = testQueryClient();
	return {
		...render(
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>,
		),
		queryClient,
	};
}
