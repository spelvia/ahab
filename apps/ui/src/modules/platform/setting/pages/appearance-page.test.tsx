import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { AppearancePage } from "@/modules/platform/setting/pages/appearance-page";
import { changeAppearance } from "@/shared/appearance";

const kept = () => JSON.parse(localStorage.getItem("ahab.preferences.v1") ?? "{}").appearance;

describe("AppearancePage", () => {
	it("applies a theme, a text size and less motion at once, and keeps them", async () => {
		changeAppearance({ theme: "dark", textPercent: 100, motion: "system" });
		render(<AppearancePage />);
		const root = document.documentElement;
		expect(root.classList.contains("dark")).toBe(true);

		await userEvent.click(
			within(screen.getByRole("radiogroup", { name: "Theme" })).getByRole("radio", {
				name: /Light/,
			}),
		);
		expect(root.classList.contains("dark")).toBe(false);
		expect(root.style.colorScheme).toBe("light");
		expect(
			within(screen.getByRole("radiogroup", { name: "Theme" })).getByRole("radio", {
				name: /Light/,
			}),
		).toHaveAttribute("aria-checked", "true");

		const size = screen.getByRole("spinbutton", { name: "Text size percentage" });
		await userEvent.clear(size);
		await userEvent.type(size, "113.5{Enter}");
		expect(root.style.fontSize).toBe("113.5%");
		expect(root.style.getPropertyValue("--ahab-text-scale")).toBe("1.135");

		await userEvent.click(screen.getByRole("switch", { name: "Reduce motion" }));
		expect(root.classList.contains("reduce-motion")).toBe(true);

		expect(kept()).toMatchObject({ theme: "light", textPercent: 113.5, motion: "reduce" });

		fireEvent.change(screen.getByRole("slider"), { target: { value: "82.5" } });
		expect(root.style.fontSize).toBe("82.5%");
		await userEvent.click(screen.getByRole("button", { name: "Reset to 100%" }));
		expect(root.style.fontSize).toBe("");

		// A colour theme's colours over the stylesheet's, and Ahab's again.
		await userEvent.click(screen.getByRole("radio", { name: /Nord/ }));
		expect(root.classList.contains("dark")).toBe(true);
		expect(root.style.getPropertyValue("--background")).toBe("#2e3440");
		await userEvent.click(screen.getByRole("radio", { name: /^Ahab/ }));
		expect(root.style.getPropertyValue("--background")).toBe("");
		expect(root.classList.contains("dark")).toBe(false);
	});
});
