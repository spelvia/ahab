import { useRouter } from "@tanstack/react-router";
import { useCallback, useEffect, useState } from "react";

import {
	pushCalls,
	usePushSubscriptions,
	useRefreshSubscriptions,
} from "@/modules/notifications/push/api";
import { WORKER, browserLabel, keyBytes, pushSupport } from "@/modules/notifications/push/model";
import { pushEndpoint, rememberPushEndpoint } from "@/shared/preferences/store";

function support() {
	if (typeof window === "undefined") return "unsupported" as const;
	return pushSupport({
		isSecureContext: window.isSecureContext,
		hasServiceWorker: "serviceWorker" in navigator,
		hasPushManager: "PushManager" in window,
	});
}

/** Whether this browser gets pushes, so an open tab does not notify twice. */
export function pushedHere(): boolean {
	return Boolean(pushEndpoint());
}

function remember(endpoint: string | undefined) {
	if (pushEndpoint() !== endpoint) rememberPushEndpoint(endpoint);
}

async function browserSubscription() {
	const registration = await navigator.serviceWorker.getRegistration("/");
	return (await registration?.pushManager.getSubscription()) ?? undefined;
}

export type PushState = "loading" | "off" | "on" | "blocked" | "working";

/** Web Push in this browser: whether it is on, and turning it on and off. */
export function usePushHere() {
	const supported = support();
	const subscriptions = usePushSubscriptions();
	const refresh = useRefreshSubscriptions();
	const [endpoint, setEndpoint] = useState<string | undefined>();
	const [state, setState] = useState<PushState>("loading");
	const [error, setError] = useState<string | undefined>();

	useEffect(() => {
		if (supported !== "supported") return;
		void browserSubscription().then((found) => {
			setEndpoint(found?.endpoint);
			setState(Notification.permission === "denied" ? "blocked" : found ? "on" : "off");
		});
	}, [supported]);

	const mine = subscriptions.data?.find((subscription) => subscription.endpoint === endpoint);
	// Removed from another device: this browser is no longer told.
	const on = state === "on" && Boolean(mine);
	useEffect(() => {
		if (subscriptions.data) remember(on ? endpoint : undefined);
	}, [on, endpoint, subscriptions.data]);

	const turnOn = useCallback(async () => {
		setError(undefined);
		setState("working");
		try {
			const permission = await Notification.requestPermission();
			if (permission !== "granted") {
				setState(permission === "denied" ? "blocked" : "off");
				return;
			}
			const registration = await navigator.serviceWorker.register(WORKER, { scope: "/" });
			await navigator.serviceWorker.ready;
			const { publicKey } = await pushCalls.pushKey();
			const key = keyBytes(publicKey);
			let found = await registration.pushManager.getSubscription();
			// Subscribed with another server's key (a server set up again): start over.
			const old = found?.options.applicationServerKey;
			if (found && old && !sameBytes(new Uint8Array(old), key)) {
				await found.unsubscribe();
				found = null;
			}
			found ??= await registration.pushManager.subscribe({
				userVisibleOnly: true,
				applicationServerKey: key,
			});
			const json = found.toJSON();
			await pushCalls.createSubscription({
				endpoint: found.endpoint,
				keys: { p256dh: json.keys?.p256dh ?? "", auth: json.keys?.auth ?? "" },
				label: browserLabel(navigator.userAgent),
			});
			setEndpoint(found.endpoint);
			remember(found.endpoint);
			setState("on");
			refresh();
		} catch (failure) {
			setError(failure instanceof Error ? failure.message : String(failure));
			setState("off");
		}
	}, [refresh]);

	const turnOff = useCallback(async () => {
		setError(undefined);
		setState("working");
		try {
			if (mine) await pushCalls.deleteSubscription(mine.id);
			await (await browserSubscription())?.unsubscribe();
		} catch (failure) {
			setError(failure instanceof Error ? failure.message : String(failure));
		}
		setEndpoint(undefined);
		remember(undefined);
		setState("off");
		refresh();
	}, [mine, refresh]);

	return {
		supported,
		state: state === "on" && !on && subscriptions.data ? ("off" as const) : state,
		mine,
		others: (subscriptions.data ?? []).filter((subscription) => subscription !== mine),
		error,
		turnOn,
		turnOff,
	};
}

function sameBytes(left: Uint8Array, right: Uint8Array) {
	return left.length === right.length && left.every((byte, at) => byte === right[at]);
}

/** Opens the page a clicked push is about, in the tab the service worker focused. */
export function usePushNavigation() {
	const router = useRouter();
	useEffect(() => {
		if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
		const listen = (event: MessageEvent) => {
			const data = event.data as { type?: string; path?: string } | undefined;
			if (data?.type === "ahab.open" && typeof data.path === "string" && data.path.startsWith("/"))
				void router.navigate({ href: data.path });
		};
		navigator.serviceWorker.addEventListener("message", listen);
		return () => navigator.serviceWorker.removeEventListener("message", listen);
	}, [router]);
}
