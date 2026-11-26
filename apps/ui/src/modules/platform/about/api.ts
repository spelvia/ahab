import { useHealth } from "@/shared/api/generated/platform-about/platform-about";

/** The server's Ahab version (also what its web app and its own daemon run). */
export function useServerVersion() {
	const health = useHealth({ query: { staleTime: 60_000 } });
	return health.data?.version;
}
