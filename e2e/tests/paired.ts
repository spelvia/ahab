import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { test as base } from "@playwright/test";

/**
 * Tests run in a browser paired with the server: with this
 * computer's credential, which the server writes into its data folder,
 * as the cookie a paired browser has.
 */
export const test = base.extend<{ paired: undefined }>({
	paired: [
		async ({ context, baseURL }, use) => {
			const file = fileURLToPath(new URL("../../target/e2e/credential", import.meta.url));
			const credential = readFileSync(file, "utf8").trim();
			await context.addCookies([
				{
					name: "ahab_device",
					value: credential,
					url: baseURL ?? "http://127.0.0.1",
					httpOnly: true,
					sameSite: "Lax",
				},
			]);
			await use(undefined);
		},
		{ auto: true },
	],
});

export { expect } from "@playwright/test";
