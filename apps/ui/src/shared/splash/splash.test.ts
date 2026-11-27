import { afterEach, describe, expect, it, vi } from "vitest";

import { hideSplash } from "@/shared/splash/splash";

function mount(logoOpacity: string) {
	document.body.innerHTML = `<div id="splash"><div class="splash-mark"></div><p id="splash-status"></p></div><div id="root"></div>`;
	const logo = document.querySelector("#splash .splash-mark") as HTMLElement;
	logo.style.opacity = logoOpacity;
}

function reduce(reduced: boolean) {
	window.matchMedia = vi.fn<(query: string) => MediaQueryList>().mockImplementation(
		(query) =>
			({
				matches: reduced && query.includes("reduce"),
				media: query,
				addEventListener: () => {},
				removeEventListener: () => {},
			}) as unknown as MediaQueryList,
	);
}

describe("the splash", () => {
	afterEach(() => {
		vi.useRealTimers();
		document.body.innerHTML = "";
	});

	it("fades out once the logo shows, then goes", () => {
		vi.useFakeTimers();
		reduce(false);
		mount("1");
		hideSplash();
		const splash = document.getElementById("splash");
		expect(splash?.classList.contains("splash-done")).toBe(true);
		vi.advanceTimersByTime(300);
		expect(document.getElementById("splash")?.hidden ?? true).toBe(true);
		expect(document.getElementById("root")).not.toBeNull();
	});

	it("goes at once when the start was quick: the logo never showed", () => {
		reduce(false);
		mount("0");
		hideSplash();
		expect(document.getElementById("splash")?.hidden ?? true).toBe(true);
	});

	it("does not fade with reduced motion", () => {
		reduce(true);
		mount("1");
		hideSplash();
		expect(document.getElementById("splash")?.hidden ?? true).toBe(true);
	});

	it("can be hidden twice, and without a splash", () => {
		reduce(false);
		mount("0");
		hideSplash();
		hideSplash();
		expect(document.getElementById("splash")?.hidden ?? true).toBe(true);
	});
});
