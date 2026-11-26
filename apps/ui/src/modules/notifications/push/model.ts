/** Web Push in this browser: what it can do, and how Settings names it. */

import { t } from "@/shared/i18n";

/** Where the service worker is served from; its scope is the whole app. */
export const WORKER = "/push-worker.js";

export type PushSupport = "supported" | "insecure" | "unsupported";

/** Whether this browser can get pushes here: Web Push needs HTTPS (or localhost). */
export function pushSupport(scope: {
	isSecureContext: boolean;
	hasServiceWorker: boolean;
	hasPushManager: boolean;
}): PushSupport {
	if (!scope.hasServiceWorker || !scope.hasPushManager) {
		return scope.isSecureContext ? "unsupported" : "insecure";
	}
	return scope.isSecureContext ? "supported" : "insecure";
}

/** "Firefox on Linux", from a user agent string. */
export function browserLabel(userAgent: string): string {
	const browser = /Edg\//.test(userAgent)
		? "Edge"
		: /OPR\/|Opera/.test(userAgent)
			? "Opera"
			: /Firefox\//.test(userAgent)
				? "Firefox"
				: /Chrome\//.test(userAgent)
					? "Chrome"
					: /Safari\//.test(userAgent)
						? "Safari"
						: t("push.browser", { ns: "notifications" });
	const system = /Android/.test(userAgent)
		? "Android"
		: /iPhone|iPad|iPod/.test(userAgent)
			? "iOS"
			: /Mac OS X|Macintosh/.test(userAgent)
				? "macOS"
				: /Windows/.test(userAgent)
					? "Windows"
					: /CrOS/.test(userAgent)
						? "ChromeOS"
						: /Linux/.test(userAgent)
							? "Linux"
							: undefined;
	return system ? t("push.browserOn", { ns: "notifications", browser, system }) : browser;
}

/** The server's key as `applicationServerKey` wants it. */
export function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
	const padded = base64url.replaceAll("-", "+").replaceAll("_", "/");
	const text = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
	const bytes = new Uint8Array(new ArrayBuffer(text.length));
	for (let at = 0; at < text.length; at += 1) bytes[at] = text.charCodeAt(at);
	return bytes;
}
