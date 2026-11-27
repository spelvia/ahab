import { useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";

import { useInbox, useNotificationSettings } from "@/modules/notifications/notification/api";
import {
	currentNotificationSound,
	useOpenTarget,
} from "@/modules/notifications/notification/hooks";
import { placeOf, showing } from "@/modules/notifications/notification/model";
import { useNotificationEvents } from "@/modules/notifications/notification/stream";
import { tellWorking, useBackground, usePhoneApp } from "@/modules/notifications/phone";
import { pushedHere, usePushNavigation } from "@/modules/notifications/push";
import type { Notification as AhabNotification } from "@/shared/api/generated/ahab.schemas";
import { usePaired } from "@/shared/api/pairing";
import { app } from "@/shared/desktop/bridge";
import { useTranslation } from "@/shared/i18n";
import {
	askedPhoneNotifications,
	rememberAskedPhoneNotifications,
} from "@/shared/preferences/store";
import { playTone } from "@/shared/sound";
import { toast } from "@/shared/ui/toast";

/** Whether this browser may show notifications outside the page. */
function browserMayNotify() {
	return (
		typeof window !== "undefined" &&
		"Notification" in window &&
		window.Notification.permission === "granted"
	);
}

/**
 * New notifications as they come: a toast in the app, unless
 * its session is already in front; outside the app when that is allowed
 * (the desktop app natively, a browser through its notifications); and
 * the unread count on the desktop app's tray; and, in the phone app, how
 * many agents work, so it stays connected while they do. Mounted once.
 */
export function NotificationAlerts() {
	const { t } = useTranslation("notifications");
	const paired = usePaired();
	const openTarget = useOpenTarget();
	usePushNavigation();
	const path = useRouterState({ select: (state) => state.location.pathname });
	const settings = useNotificationSettings();
	const unread = useInbox().data?.unread;
	const outside = settings.data?.outside ?? true;

	useEffect(() => {
		if (unread !== undefined) void app.badge(unread);
	}, [unread]);

	// The phone app: keeps this server for its background
	// connection, and asks once, after pairing, to tell about agents.
	const phone = usePhoneApp();
	const background = useBackground(phone);
	const permission = background.status?.permission;
	const ask = background.ask;
	useEffect(() => {
		if (!phone || !paired || permission !== "prompt" || askedPhoneNotifications()) return;
		rememberAskedPhoneNotifications();
		toast(t("alerts.askPhone"), {
			action: { label: t("alerts.allow"), run: () => void ask({ action: "allow" }) },
			duration: 20_000,
		});
	}, [phone, paired, permission, ask, t]);

	useNotificationEvents(
		paired,
		(notification: AhabNotification) => {
			const visible = typeof document !== "undefined" && document.visibilityState === "visible";
			const focused = visible && document.hasFocus();
			if (focused && showing(path, notification.target)) return;
			// This device's sound (Settings → Notifications), for every kind shown.
			const sound = currentNotificationSound();
			if (sound.on) playTone(sound.tone, sound.volume);
			// Outside the app only as its kind allows (shown inside, always).
			const goesOutside =
				outside &&
				(settings.data?.kinds.find((kind) => kind.id === notification.kind)?.outside ?? true);
			const open = () => openTarget(notification.target);
			// In the desktop app, a toast in a window in the background is shown
			// natively too (shared/ui/toast).
			const place = placeOf(notification);
			toast(notification.title, {
				body: notification.body ?? undefined,
				detail: place,
				tone: notification.level,
				native: goesOutside,
				action: { label: t("alerts.open"), run: open },
				duration: 6000,
			});
			// A browser that gets pushes is told by its service worker instead.
			// Not in front (hidden, or another window has the focus): the
			// system's notification too.
			if (goesOutside && !focused && browserMayNotify() && !pushedHere()) {
				const shown = new window.Notification(notification.title, {
					body: [notification.body, place].filter(Boolean).join("\n") || undefined,
					tag: notification.id,
				});
				shown.addEventListener("click", () => {
					window.focus();
					open();
					shown.close();
				});
			}
		},
		phone ? tellWorking : undefined,
	);
	return null;
}
