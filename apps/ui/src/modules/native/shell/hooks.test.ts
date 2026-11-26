import { afterEach, describe, expect, it } from "vitest";

import { isBuiltinScreen } from "@/modules/native/shell/hooks";

/** The name the app sets before any script runs (apps/native/src/windows.rs). */
const APP = "__AHAB_APP__";
const page = window as unknown as Record<string, unknown>;

function at(url: string, app: boolean) {
	window.history.replaceState(null, "", "/");
	Object.defineProperty(window, "location", { value: new URL(url), configurable: true });
	if (app) Object.defineProperty(window, APP, { value: { shell: "tauri" }, configurable: true });
	else delete page[APP];
}

const original = window.location;

describe("the app's own screens", () => {
	afterEach(() => {
		Object.defineProperty(window, "location", { value: original, configurable: true });
		delete page[APP];
	});

	it("show only in the app's own copy of the web app", () => {
		at("tauri://localhost/index.html#/welcome", true);
		expect(isBuiltinScreen()).toBe(true);
		at("http://tauri.localhost/index.html", true);
		expect(isBuiltinScreen()).toBe(true);
	});

	it("never in a browser, nor on a server's page inside the app", () => {
		at("tauri://localhost/index.html", false);
		expect(isBuiltinScreen()).toBe(false);
		at("https://ahab.example.internal/", false);
		expect(isBuiltinScreen()).toBe(false);
		at("https://ahab.example.internal/#/welcome", true);
		expect(isBuiltinScreen()).toBe(false);
	});
});
