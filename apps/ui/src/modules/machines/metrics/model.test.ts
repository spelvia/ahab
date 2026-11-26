import { describe, expect, it } from "vitest";

import { bytes, cpuTime, duration, share, toneOf } from "@/modules/machines/metrics/model";

describe("resources", () => {
	it("reads sizes, shares and durations as people do", () => {
		expect(bytes(512)).toBe("512 B");
		expect(bytes(10407 * 1024 * 1024)).toBe("10.2 GB");
		expect(bytes(915 * 1024 ** 3)).toBe("915 GB");
		expect(share(1, 4)).toBe(25);
		expect(share(1, 0)).toBe(0);
		expect(duration(45 * 60)).toBe("45 min");
		expect(duration(5 * 3600 + 12 * 60)).toBe("5 h 12 min");
		expect(duration(3 * 86400 + 4 * 3600)).toBe("3 d 4 h");
		expect([toneOf(10), toneOf(80), toneOf(95)]).toEqual(["ok", "warning", "danger"]);
		expect([cpuTime(12_340), cpuTime(245_000), cpuTime(11_520_000)]).toEqual([
			"12.3s",
			"4:05",
			"3h 12m",
		]);
	});
});
