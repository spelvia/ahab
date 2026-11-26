/*
 * Ahab's service worker, for Web Push only: it shows what the
 * server pushes when no Ahab tab is in front, and opens its page when
 * clicked. It caches nothing.
 */

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

function read(event) {
	if (!event.data) return {};
	try {
		return event.data.json();
	} catch {
		return { title: "Ahab", body: event.data.text() };
	}
}

self.addEventListener("push", (event) => {
	const message = read(event);
	event.waitUntil(
		(async () => {
			const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
			// A tab in front shows it as a toast already; a test always shows.
			const inFront = windows.some(
				(window) => window.focused && window.visibilityState === "visible",
			);
			if (inFront && message.kind !== "ahab.test") return;
			await self.registration.showNotification(message.title || "Ahab", {
				body: message.body || undefined,
				tag: message.tag,
				icon: "/app-192.png",
				badge: "/app-192.png",
				data: { path: message.path, url: message.url },
			});
		})(),
	);
});

self.addEventListener("notificationclick", (event) => {
	event.notification.close();
	const { path, url } = event.notification.data || {};
	event.waitUntil(
		(async () => {
			if (url) return self.clients.openWindow(url);
			const target = new URL(path || "/", self.location.origin);
			const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
			const open = windows.find((window) => new URL(window.url).origin === target.origin);
			if (open) {
				await open.focus();
				// A worker's Client.postMessage has no target origin: it reaches only that tab.
				// oxlint-disable-next-line unicorn/require-post-message-target-origin
				open.postMessage({ type: "ahab.open", path: target.pathname + target.search });
				return undefined;
			}
			return self.clients.openWindow(target.href);
		})(),
	);
});
