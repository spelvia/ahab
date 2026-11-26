import {
	createSubscription,
	deleteSubscription,
	pushKey,
	useDeleteSubscription,
	useListSubscriptions,
	useTestSubscription,
} from "@/shared/api/generated/notifications-push/notifications-push";
import { useInvalidate } from "@/shared/api/invalidate";

/** The browsers that get pushes. */
export function usePushSubscriptions() {
	return useListSubscriptions();
}

export function useRefreshSubscriptions() {
	return useInvalidate("/push-subscriptions");
}

export function useRemoveSubscription() {
	const refresh = useRefreshSubscriptions();
	return useDeleteSubscription({ mutation: { onSuccess: refresh } });
}

export function useTestPush() {
	return useTestSubscription();
}

/** The calls turning Web Push on and off, outside React Query. */
export const pushCalls = { createSubscription, deleteSubscription, pushKey };
