import {
	useClearNotifications,
	useGetNotificationSettings,
	useListNotifications,
	useReadAllNotifications,
	useReadNotification,
	useSetNotificationSettings,
} from "@/shared/api/generated/notifications-notification/notifications-notification";
import { useInvalidate } from "@/shared/api/invalidate";

/** The inbox's newest notifications and the unread count. */
export function useInbox() {
	return useListNotifications({ limit: 50 });
}

export function useNotificationSettings() {
	return useGetNotificationSettings();
}

/** Refreshes the inbox and the settings after a change. */
export function useRefreshNotifications() {
	return useInvalidate("/notifications", "/notification-settings");
}

export function useMarkRead() {
	const refresh = useRefreshNotifications();
	return useReadNotification({ mutation: { onSuccess: refresh } });
}

export function useMarkAllRead() {
	const refresh = useRefreshNotifications();
	return useReadAllNotifications({ mutation: { onSuccess: refresh } });
}

export function useClearRead() {
	const refresh = useRefreshNotifications();
	return useClearNotifications({ mutation: { onSuccess: refresh } });
}

export function useSaveNotificationSettings() {
	const refresh = useRefreshNotifications();
	return useSetNotificationSettings({ mutation: { onSuccess: refresh } });
}
