import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, test } from "./paired";

// `ahab up --foreground` runs the daemon by default, so this computer is a
// machine, connected to its own server.
test("first run opens a folder, and the workbench shows its files", async ({ page }) => {
	const folder = mkdtempSync(join(tmpdir(), "ahab-e2e-"));
	const app = join(folder, "demo");
	mkdirSync(join(app, "src"), { recursive: true });
	writeFileSync(join(app, "src", "main.rs"), "fn main() {}\n");

	await page.goto("/");
	await expect(page.getByRole("heading", { name: "Open a folder to start" })).toBeVisible();

	// Through the API, since the picker starts in the home folder.
	const opened = await page.request.post("/api/v1/projects/open", { data: { path: app } });
	expect(opened.ok()).toBeTruthy();
	const { project } = (await opened.json()) as { project: { id: string; name: string } };
	expect(project.name).toBe("demo");
	await page.goto("/");

	await expect(page).toHaveURL(new RegExp(`/p/${project.id}`));
	// Files is in the right sidebar, open by default.
	await page.getByRole("treeitem", { name: "src" }).click();
	await page.getByRole("treeitem", { name: "main.rs" }).click();
	await expect(page.getByText("fn main() {}")).toBeVisible();
});

test("Settings lists this computer as an online machine", async ({ page }) => {
	await page.goto("/settings/machines");
	await expect(page.getByRole("heading", { name: /^Machines/ })).toBeVisible();
	// This computer's row, whatever its name.
	const row = page.getByRole("row").filter({ hasText: "This server" });
	await expect(row.getByText("Online", { exact: true })).toBeVisible();
	await expect(page.getByText("1 of 1 online")).toBeVisible();
});
