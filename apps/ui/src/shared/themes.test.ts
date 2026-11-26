import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
	COLOR_THEMES,
	colorTheme,
	fontStack,
	THEME_VARIABLES,
	themeVariables,
	variantOf,
} from "@/shared/themes";

/** The variables globals.css gives its light theme (`:root { … }`). */
function stylesheetVariables(): string[] {
	const globals = readFileSync(`${process.cwd()}/src/shared/styles/globals.css`, "utf8");
	const root = /:root\s*\{([^}]*)\}/.exec(globals)?.[1] ?? "";
	return [...root.matchAll(/(--[a-z-]+)\s*:/g)]
		.map((match) => match[1] ?? "")
		.filter((name) => name !== "--radius");
}

describe("colour themes", () => {
	it("set every colour the stylesheet's own themes set", () => {
		const variables = stylesheetVariables();
		expect(variables.length).toBeGreaterThan(40);
		for (const name of variables) expect(THEME_VARIABLES).toContain(name);
	});

	it("give each variant real colours", () => {
		for (const theme of COLOR_THEMES) {
			for (const [dark, colors] of [
				[false, theme.light],
				[true, theme.dark],
			] as const) {
				if (!colors) continue;
				for (const [name, value] of Object.entries(themeVariables(colors, dark)))
					expect(value, `${theme.id} ${name}`).toMatch(/^#[0-9a-f]{6}$/i);
			}
		}
	});

	it("show the variant asked for, else the one a theme has", () => {
		const nord = colorTheme("nord");
		expect(nord && variantOf(nord, false).dark).toBe(true);
		const github = colorTheme("github");
		expect(github && variantOf(github, false).dark).toBe(false);
		expect(colorTheme("")).toBeUndefined();
	});

	it("put a font chosen by name before the usual fallbacks", () => {
		expect(fontStack("", "ui")).toBe("");
		expect(fontStack("system", "code")).toContain("ui-monospace");
		expect(fontStack(' Fira "Code" ', "code")).toMatch(/^"Fira Code", /);
	});
});
