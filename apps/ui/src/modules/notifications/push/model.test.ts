import { describe, expect, it } from "vitest";

import { browserLabel, keyBytes, pushSupport } from "@/modules/notifications/push/model";

describe("web push", () => {
	it("needs HTTPS, a service worker and a push manager", () => {
		const all = { isSecureContext: true, hasServiceWorker: true, hasPushManager: true };
		expect(pushSupport(all)).toBe("supported");
		expect(pushSupport({ ...all, isSecureContext: false })).toBe("insecure");
		expect(pushSupport({ ...all, isSecureContext: false, hasServiceWorker: false })).toBe(
			"insecure",
		);
		expect(pushSupport({ ...all, hasPushManager: false })).toBe("unsupported");
	});

	it("names browsers as people say them", () => {
		expect(
			browserLabel("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0"),
		).toBe("Firefox on Linux");
		expect(
			browserLabel(
				"Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
			),
		).toBe("Safari on iOS");
		expect(
			browserLabel(
				"Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Mobile Safari/537.36",
			),
		).toBe("Chrome on Android");
		expect(browserLabel("curl/8")).toBe("A browser");
	});

	it("reads the server's key", () => {
		expect([...keyBytes("BAEC_w")]).toEqual([4, 1, 2, 255]);
	});
});
