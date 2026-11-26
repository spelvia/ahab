import { useSyncExternalStore } from "react";

/** Phone-sized: below Tailwind's `sm` (640px), where the composer lays out for thumbs. */
const PHONE = "(max-width: 639px)";

/** Whether the window matches a media `query`, kept current. */
function useMedia(query: string): boolean {
	return useSyncExternalStore(
		(change) => {
			const list = window.matchMedia?.(query);
			list?.addEventListener("change", change);
			return () => list?.removeEventListener("change", change);
		},
		() => window.matchMedia?.(query).matches ?? false,
		() => false,
	);
}

/** Whether the window is phone-sized (see `PHONE`). */
export function usePhone(): boolean {
	return useMedia(PHONE);
}
