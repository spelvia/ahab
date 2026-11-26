import { useSyncExternalStore } from "react";

import { isLanguage, setLanguage } from "@/shared/i18n";
import { rememberAppearance, storedAppearance } from "@/shared/preferences/store";
import { colorTheme, fontStack, THEME_VARIABLES, themeVariables, variantOf } from "@/shared/themes";

export type Theme = "system" | "light" | "dark";
type Motion = "system" | "reduce";

/** How Ahab looks on this device (Settings → Appearance and → Agent editor). */
export interface Appearance {
	theme: Theme;
	textPercent: number;
	motion: Motion;
	language: string;
	/** Agents' thinking shown open in conversations. */
	expandThinking: boolean;
	/** Commands and other tool calls shown open in conversations. */
	expandTools: boolean;
	/** A chat's session cost (US dollars) shown in its Session usage. */
	showCost: boolean;
	/** The tokens a chat's session spent shown in its Session usage. */
	showTokens: boolean;
	/** A built-in colour theme (`@/shared/themes`); empty for Ahab's own. */
	colorTheme: string;
	/** The interface's font and code's: a choice's ID or a font's name; empty for Ahab's. */
	uiFont: string;
	codeFont: string;
}

export const TEXT_PERCENT = { min: 75, max: 200, step: 0.5, default: 100 } as const;
const presetPercent = { small: 87.5, default: 100, large: 112.5, larger: 125 };

export function clampTextPercent(percent: number): number {
	const stepped = Math.round(percent / TEXT_PERCENT.step) * TEXT_PERCENT.step;
	return Math.min(TEXT_PERCENT.max, Math.max(TEXT_PERCENT.min, stepped));
}

const DEFAULTS: Appearance = {
	theme: "system",
	textPercent: TEXT_PERCENT.default,
	motion: "system",
	language: "en",
	expandThinking: false,
	expandTools: false,
	showCost: true,
	showTokens: true,
	colorTheme: "",
	uiFont: "",
	codeFont: "",
};

const saved = storedAppearance();
const percent = saved.textPercent;
let current: Appearance = {
	...DEFAULTS,
	...saved,
	textPercent:
		typeof percent === "number" && Number.isFinite(percent)
			? clampTextPercent(percent)
			: (presetPercent[saved.textSize ?? "default"] ?? TEXT_PERCENT.default),
};
const listeners = new Set<() => void>();
const systemLight =
	typeof window !== "undefined" ? window.matchMedia?.("(prefers-color-scheme: light)") : undefined;

/** Whether the page shows dark now: chosen, or the system's; a theme with one variant shows it. */
function isDark(shown: Appearance = current): boolean {
	const asked = shown.theme === "system" ? !systemLight?.matches : shown.theme === "dark";
	const theme = colorTheme(shown.colorTheme);
	return theme ? variantOf(theme, asked).dark : asked;
}

/** The factor text is shown at; the editor and terminals size by it too. */
export function textScale(): number {
	return current.textPercent / 100;
}

/**
 * Puts `current` on the page: the `dark` class and colour scheme, the root
 * font size every rem follows (and `--ahab-text-scale` for what is sized in
 * pixels), `reduce-motion`, and the language.
 */
function apply() {
	const root = document.documentElement;
	const dark = isDark();
	root.classList.toggle("dark", dark);
	// A built-in theme's colours over the stylesheet's; Ahab's own sets none.
	const theme = colorTheme(current.colorTheme);
	const variables = theme ? themeVariables(variantOf(theme, dark).colors, dark) : {};
	for (const name of THEME_VARIABLES) {
		const value = variables[name];
		if (value) root.style.setProperty(name, value);
		else root.style.removeProperty(name);
	}
	root.dataset.colorTheme = theme?.id ?? "ahab";
	for (const [name, chosen, kind] of [
		["--font-sans", current.uiFont, "ui"],
		["--font-mono", current.codeFont, "code"],
	] as const) {
		const stack = fontStack(chosen, kind);
		if (stack) root.style.setProperty(name, stack);
		else root.style.removeProperty(name);
	}
	root.style.colorScheme = dark ? "dark" : "light";
	const scale = textScale();
	root.style.fontSize = scale === 1 ? "" : `${current.textPercent}%`;
	root.style.setProperty("--ahab-text-scale", String(scale));
	root.classList.toggle("reduce-motion", current.motion === "reduce");
	if (isLanguage(current.language)) setLanguage(current.language);
	for (const listener of listeners) listener();
}

/** Applies the kept appearance, and follows the system's theme while asked to. */
export function startAppearance() {
	apply();
	systemLight?.addEventListener("change", () => {
		if (current.theme === "system") apply();
	});
}

/** Changes how Ahab looks here, at once, and keeps it. */
export function changeAppearance(change: Partial<Appearance>) {
	current = { ...current, ...change };
	rememberAppearance(current);
	apply();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** The appearance, re-rendering when it changes (or the system's theme does). */
export function useAppearance(): Appearance & { dark: boolean } {
	const shown = useSyncExternalStore(subscribe, () => current);
	const dark = useSyncExternalStore(subscribe, () => isDark(shown));
	return { ...shown, dark };
}

/** Calls `listener` whenever the appearance is applied again; returns the unsubscribe. */
export function onAppearance(listener: () => void): () => void {
	return subscribe(listener);
}
