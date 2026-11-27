import { useSyncExternalStore } from "react";

import type { Keymap } from "@/shared/keymap/model";
import { keymap, subscribe } from "@/shared/keymap/store";

/** The keymap in use, kept up to date. */
export function useKeymap(): Keymap {
	return useSyncExternalStore(subscribe, keymap, keymap);
}
