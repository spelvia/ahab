import type { CoreFeature } from "@/shared/api/generated/ahab.schemas";
import {
	useListFeatures,
	useSetFeature,
} from "@/shared/api/generated/platform-feature/platform-feature";
import { useInvalidate } from "@/shared/api/invalidate";

/** Ahab's core features, and which are on. */
export function useFeatures() {
	return useListFeatures({ query: { staleTime: 60_000 } });
}

/** Whether each core feature is on; all are while they load. */
export function useFeatureFlags(): (id: CoreFeature["id"]) => boolean {
	const { data } = useFeatures();
	return (id) => data?.items.find((feature) => feature.id === id)?.enabled ?? true;
}

/** Switches a feature on or off, for everyone. */
export function useSetFeatureEnabled() {
	const refresh = useInvalidate("/features");
	return useSetFeature({ mutation: { onSettled: refresh } });
}
