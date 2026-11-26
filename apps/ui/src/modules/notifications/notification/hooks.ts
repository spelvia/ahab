import { useRouter } from "@tanstack/react-router";
import { useCallback, useSyncExternalStore } from "react";

import { targetPath, targetUrl } from "@/modules/notifications/notification/model";
import { app } from "@/shared/desktop/bridge";
import {
	type NotificationSound,
	notificationSound,
	rememberNotificationSound,
} from "@/shared/preferences/store";

/** Opens what a notification is about: a page of this app, or one outside it. */
export function useOpenTarget() {
	const router = useRouter();
	return useCallback(
		(target: unknown) => {
			const path = targetPath(target);
			if (path) {
				void router.navigate({ href: path });
				return;
			}
			const url = targetUrl(target);
			if (!url) return;
			// The desktop app opens it in the person's browser; a browser, in a new tab.
			void app.info().then((inApp) => {
				if (inApp) void app.openExternal(url);
				else window.open(url, "_blank", "noopener,noreferrer");
			});
		},
		[router],
	);
}

// --- The sound notifications make on this device -------------------------------

let sound = notificationSound();
const soundListeners = new Set<() => void>();

/** This device's notification sound, re-rendering when it changes. */
export function useNotificationSound(): NotificationSound {
	return useSyncExternalStore(
		(listener) => {
			soundListeners.add(listener);
			return () => soundListeners.delete(listener);
		},
		() => sound,
	);
}

/** The sound as it is now, outside React (as a notification arrives). */
export function currentNotificationSound(): NotificationSound {
	return sound;
}

export function changeNotificationSound(change: Partial<NotificationSound>) {
	sound = { ...sound, ...change };
	rememberNotificationSound(sound);
	for (const listener of soundListeners) listener();
}
