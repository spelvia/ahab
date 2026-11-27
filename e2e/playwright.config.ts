import { defineConfig, devices } from "@playwright/test";

// `make e2e` prepares what the server needs: a fresh config and data folder
// (it removes the old one), the binary and the built UI.
const port = Number(process.env.AHAB_E2E_PORT ?? 8191);
const baseUrl = `http://127.0.0.1:${port}`;

export default defineConfig({
	testDir: "tests",
	timeout: 60_000,
	expect: { timeout: 10_000 },
	fullyParallel: false,
	workers: 1,
	reporter: process.env.CI ? "list" : [["list"], ["html", { open: "never" }]],
	use: {
		baseURL: baseUrl,
		trace: "retain-on-failure",
	},
	projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
	webServer: {
		command: "target/debug/ahab up --foreground",
		cwd: "..",
		url: `${baseUrl}/api/v1/health`,
		reuseExistingServer: false,
		stdout: "pipe",
		timeout: 60_000,
		env: {
			...(process.env as Record<string, string>),
			AHAB_CONFIG_DIR: "target/e2e",
			AHAB_DATA_DIR: "target/e2e",
			AHAB_LISTEN_ADDR: `127.0.0.1:${port}`,
			RUST_LOG: "info",
		},
	},
});
