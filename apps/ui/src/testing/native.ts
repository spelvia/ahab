import { vi } from "vitest";

import type { Boot, ThisComputer } from "@/modules/native";

/** This computer as a test describes it. */
export function computer(overrides: Partial<ThisComputer> = {}): ThisComputer {
	return {
		role: "none",
		installed: null,
		bundled: { path: "/opt/ahab/ahab", version: "0.1.0" },
		serviceManager: "systemd",
		managed: false,
		serviceInstalled: false,
		running: false,
		...overrides,
	};
}

export function boot(overrides: Partial<Boot> = {}): Boot {
	return {
		servers: { servers: [], last: null, store: "keychain", storeNote: null },
		computer: computer(),
		settings: {
			keepInTray: null,
			globalShortcut: null,
			notifications: true,
			checkForUpdates: true,
		},
		version: "0.1.0",
		...overrides,
	};
}

/** Every call into the app, as mocks (`vi.mock("@/modules/native/shell/api")` in each test file). */
export function mockNative(
	native: Record<string, unknown>,
	values: Partial<Record<string, unknown>> = {},
) {
	for (const key of Object.keys(native)) {
		(native as Record<string, ReturnType<typeof vi.fn>>)[key] = vi.fn<() => Promise<unknown>>(
			async () => values[key],
		);
	}
}
