import { useSyncExternalStore } from "react";

import {
	type Contributions,
	contributions,
	subscribe,
} from "@/modules/plugins/runtime/components/registry";
import { currentHost, type Host, subscribeHost } from "@/modules/plugins/runtime/model";

/** What plugins add to the web app, kept up to date. */
export function useContributions(): Contributions {
	return useSyncExternalStore(subscribe, contributions);
}

/** The workbench as plugins see it, kept up to date. */
export function useHost(): Host {
	return useSyncExternalStore(subscribeHost, currentHost);
}
