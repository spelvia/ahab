import { afterEach, describe, expect, it, vi } from "vitest";

type Bridge = typeof import("@/shared/desktop/bridge");

const INTERNALS = "__TAURI_INTERNALS__";

async function load(
	invoke?: (command: string, args?: unknown) => Promise<unknown>,
): Promise<Bridge> {
	vi.resetModules();
	if (invoke) Reflect.set(window, INTERNALS, { invoke });
	else Reflect.deleteProperty(window, INTERNALS);
	return import("@/shared/desktop/bridge");
}

const info = {
	app: true,
	version: "0.1.0",
	platform: "linux",
	server: { id: "s1", name: "atlas", url: "https://atlas.lan", local: false },
	functions: ["info"],
};

describe("the desktop app's bridge", () => {
	afterEach(() => {
		Reflect.deleteProperty(window, INTERNALS);
	});

	it("is absent in a browser, where every function does nothing", async () => {
		const { app } = await load();
		expect(await app.info()).toBeUndefined();
		expect(await app.servers()).toEqual([]);
		expect(await app.notify({ title: "Done" })).toBeUndefined();
	});

	it("detects the app and calls its functions", async () => {
		const invoke = vi.fn<(command: string) => Promise<unknown>>(async (command) => {
			if (command === "bridge_info") return info;
			if (command === "bridge_servers") return [{ id: "s1", name: "atlas", current: true }];
			return null;
		});
		const { app } = await load(invoke);
		expect(await app.info()).toEqual(info);
		expect(await app.servers()).toHaveLength(1);
		await app.badge(2.4);
		await app.newWindow("/p/x");
		expect(invoke).toHaveBeenCalledWith("bridge_badge", { count: 2 });
		expect(invoke).toHaveBeenCalledWith("bridge_new_window", { path: "/p/x" });
	});

	it("stays quiet when the app does not let this page in", async () => {
		const invoke = vi.fn<() => Promise<unknown>>(async () => {
			throw new Error("not_allowed");
		});
		const { app } = await load(invoke);
		expect(await app.info()).toBeUndefined();
		await app.switchServer("s2");
		expect(invoke).toHaveBeenCalledTimes(1);
	});
});
