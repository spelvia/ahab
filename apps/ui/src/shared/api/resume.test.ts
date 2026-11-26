import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { keepOpen, onResume, retryDelay } from "@/shared/api/resume";

let visibility: DocumentVisibilityState = "visible";

function show(state: DocumentVisibilityState) {
	visibility = state;
	document.dispatchEvent(new Event("visibilitychange"));
}

beforeEach(() => {
	visibility = "visible";
	Object.defineProperty(document, "visibilityState", {
		configurable: true,
		get: () => visibility,
	});
});

afterEach(() => {
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

it("waits at most about 5 seconds while the page is shown, and 30 while hidden", () => {
	expect(retryDelay(0)).toBeLessThanOrEqual(1_200);
	expect(retryDelay(20)).toBeLessThanOrEqual(6_000);
	expect(retryDelay(20)).toBeGreaterThanOrEqual(4_000);
	visibility = "hidden";
	expect(retryDelay(20)).toBeGreaterThanOrEqual(24_000);
});

it("says the page is back, and whether its connections may be dead", () => {
	vi.useFakeTimers();
	const heard: boolean[] = [];
	const stop = onResume((stale) => heard.push(stale));
	show("hidden");
	vi.advanceTimersByTime(3_000);
	show("visible");
	show("hidden");
	vi.advanceTimersByTime(60_000);
	show("visible");
	window.dispatchEvent(new Event("online"));
	expect(heard).toEqual([false, true, true]);
	stop();
	window.dispatchEvent(new Event("online"));
	expect(heard).toHaveLength(3);
});

it("opens a stream again when the page is back and it was closed or may be dead", () => {
	vi.useFakeTimers();
	const made: { readyState: number; close: () => void }[] = [];
	vi.stubGlobal(
		"EventSource",
		Object.assign(
			class {
				readyState = 1;
				constructor() {
					made.push(this);
				}
				close() {
					this.readyState = 2;
				}
			},
			{ CONNECTING: 0, OPEN: 1, CLOSED: 2 },
		),
	);
	const stream = keepOpen(() => new EventSource("/events"));
	show("hidden");
	show("visible");
	expect(made).toHaveLength(1);
	made[0]?.close();
	show("hidden");
	show("visible");
	expect(made).toHaveLength(2);
	show("hidden");
	vi.advanceTimersByTime(60_000);
	show("visible");
	expect(made).toHaveLength(3);
	expect(made[1]?.readyState).toBe(2);
	stream.close();
	window.dispatchEvent(new Event("online"));
	expect(made).toHaveLength(3);
});
