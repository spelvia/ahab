import { useSyncExternalStore } from "react";

/** Whether the window is phone-sized (below Tailwind's `md`), kept current. */
export function useIsPhone(): boolean {
	return useSyncExternalStore(
		(change) => {
			const query = window.matchMedia?.("(max-width: 767px)");
			query?.addEventListener("change", change);
			return () => query?.removeEventListener("change", change);
		},
		() => window.matchMedia?.("(max-width: 767px)").matches ?? false,
		() => false,
	);
}
