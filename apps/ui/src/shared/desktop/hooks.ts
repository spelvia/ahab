import { useQuery } from "@tanstack/react-query";

import { app } from "@/shared/desktop/bridge";

/** The desktop app around this page, or undefined in a browser. */
export function useDesktopApp() {
	const { data } = useQuery({
		queryKey: ["desktop-app", "info"],
		queryFn: async () => (await app.info()) ?? null,
		staleTime: Number.POSITIVE_INFINITY,
		retry: false,
	});
	return data ?? undefined;
}

/** The app's servers, for the switcher; empty in a browser. */
export function useAppServers(enabled: boolean) {
	return useQuery({
		queryKey: ["desktop-app", "servers"],
		queryFn: app.servers,
		enabled,
		retry: false,
	});
}
