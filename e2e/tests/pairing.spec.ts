import { expect, test } from "@playwright/test";

// A browser the server does not know sees the pairing screen, with the code
// a paired client approves.
test("a browser that is not paired sees the pairing screen", async ({ page }) => {
	await page.goto("/");
	await expect(page.getByRole("heading", { name: "Your agents, on your machines." })).toBeVisible();
	await expect(page.getByLabel("Pairing code")).toHaveText(/^[0-9A-Z]{4}$/);
});
