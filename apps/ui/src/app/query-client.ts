import { QueryClient } from "@tanstack/react-query";

import { ApiError } from "@/shared/api/fetch";

export const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			staleTime: 30_000,
			retry: (attempt, error) =>
				attempt === 0 && error instanceof ApiError && (error.status === 0 || error.status >= 500),
		},
	},
});
