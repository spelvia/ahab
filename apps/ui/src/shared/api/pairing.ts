import { useSyncExternalStore } from "react";

/**
 * Whether the server knows this browser: it turns false when any
 * request answers `401 not_paired`, and true again once the browser is
 * paired. The app shows the pairing screen while it is false.
 */
let paired = true;
const listeners = new Set<() => void>();

function set(value: boolean) {
	if (paired === value) return;
	paired = value;
	for (const listener of listeners) listener();
}

/** Called by the API client when the server answered `not_paired`. */
export function markNotPaired() {
	set(false);
}

/** Called once this browser is paired. */
export function markPaired() {
	set(true);
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

/** Whether this browser is paired, as far as the last answers said. */
export function usePaired() {
	return useSyncExternalStore(
		subscribe,
		() => paired,
		() => paired,
	);
}
