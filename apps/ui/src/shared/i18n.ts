import { createInstance } from "i18next";
import { initReactI18next } from "react-i18next";

import { LANGUAGES, type Language, resources } from "@/shared/i18n/resources";

/**
 * The app's texts (i18next). Components use `useTranslation("<namespace>")`
 * and `t("key")`; code outside React uses `t` from here. Numbers, dates and
 * lists go through the `format*` helpers, so they follow the language too.
 */
const i18next = createInstance();
void i18next.use(initReactI18next).init({
	resources,
	lng: "en",
	fallbackLng: "en",
	defaultNS: "common",
	ns: Object.keys(resources.en),
	// React escapes on its own.
	interpolation: { escapeValue: false },
	returnNull: false,
	// Everything is bundled: ready at once, no Suspense.
	initAsync: false,
	react: { useSuspense: false },
});

export const i18n = i18next;
export const t = i18next.t.bind(i18next);
export { Trans, useTranslation } from "react-i18next";
export { LANGUAGES, type Language } from "@/shared/i18n/resources";

/** Switches the app's language; texts update where they are shown. */
export function setLanguage(language: Language) {
	void i18next.changeLanguage(language);
	document.documentElement.lang = language;
}

/** The language for `Intl`: the app's, which may differ from the browser's. */
function locale(): string {
	return i18next.language || "en";
}

/** A number as the language writes it: `1,234.5`. */
export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
	return new Intl.NumberFormat(locale(), options).format(value);
}

/** A date and/or time as the language writes it. */
export function formatDate(
	value: Date | string | number,
	options?: Intl.DateTimeFormatOptions,
): string {
	return new Intl.DateTimeFormat(locale(), options).format(new Date(value));
}

/** "a, b and c" as the language writes it. */
export function formatList(items: string[], type: Intl.ListFormatType = "conjunction"): string {
	return new Intl.ListFormat(locale(), { style: "long", type }).format(items);
}

export function isLanguage(code: string): code is Language {
	return LANGUAGES.some((language) => language.code === code);
}
