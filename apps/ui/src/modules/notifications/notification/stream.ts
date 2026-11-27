import { useEffect, useRef } from "react";

import { useRefreshNotifications } from "@/modules/notifications/notification/api";
import type { Notification as AhabNotification } from "@/shared/api/generated/ahab.schemas";
import { keepOpen } from "@/shared/api/resume";

type Heard =
	| { type: "created"; notification: AhabNotification }
	| { type: "changed"; unread?: number }
	| { type: "activity"; working: number };

/**
 * Keeps the inbox current as the server announces notifications,
 * and tells `onCreated` about each new one.
 */
export function useNotificationEvents(
	enabled: boolean,
	onCreated: (notification: AhabNotification) => void,
	/** How many agents work: at once, then every 20 seconds. */
	onActivity?: (working: number) => void,
) {
	const refresh = useRefreshNotifications();
	const latest = useRef({ refresh, onCreated, onActivity });
	useEffect(() => {
		latest.current = { refresh, onCreated, onActivity };
	});
	useEffect(() => {
		if (!enabled || typeof EventSource === "undefined") return;
		const stream = keepOpen(() => {
			const source = new EventSource("/api/v1/notification-events");
			source.addEventListener("open", () => latest.current.refresh());
			source.addEventListener("message", (message) => {
				const heard = JSON.parse(String(message.data)) as Heard;
				if (heard.type === "activity") {
					latest.current.onActivity?.(heard.working);
					return;
				}
				latest.current.refresh();
				if (heard.type === "created") latest.current.onCreated(heard.notification);
			});
			return source;
		});
		return () => stream.close();
	}, [enabled]);
}
