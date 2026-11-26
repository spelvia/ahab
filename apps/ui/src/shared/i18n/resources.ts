import clients from "@/shared/i18n/locales/en/clients.json";
import common from "@/shared/i18n/locales/en/common.json";
import machines from "@/shared/i18n/locales/en/machines.json";
import native from "@/shared/i18n/locales/en/native.json";
import notifications from "@/shared/i18n/locales/en/notifications.json";
import platform from "@/shared/i18n/locales/en/platform.json";
import plugins from "@/shared/i18n/locales/en/plugins.json";
import projects from "@/shared/i18n/locales/en/projects.json";
import sessions from "@/shared/i18n/locales/en/sessions.json";
import shell from "@/shared/i18n/locales/en/shell.json";
import tools from "@/shared/i18n/locales/en/tools.json";

/**
 * Every text of the app, by language and namespace. A namespace is a module's
 * (`sessions`, `projects`, …), the app shell's (`shell`) or the shared parts'
 * (`common`). English is the source; another language is a folder beside it
 * with the same files, and its missing keys fall back to English.
 */
export const resources = {
	en: {
		common,
		shell,
		clients,
		machines,
		native,
		notifications,
		platform,
		plugins,
		projects,
		sessions,
		tools,
	},
} as const;

/** The languages Ahab has, with their own names, for the language setting. */
export const LANGUAGES = [{ code: "en", name: "English" }] as const;

export type Language = (typeof LANGUAGES)[number]["code"];
