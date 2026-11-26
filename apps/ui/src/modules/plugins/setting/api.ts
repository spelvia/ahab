import type { PluginSettingsScope } from "@/shared/api/generated/ahab.schemas";
import {
	pluginSettings,
	useEffectivePluginSettings,
	usePluginSettings,
	useSavePluginSettings,
} from "@/shared/api/generated/plugins-setting/plugins-setting";
import { useInvalidate } from "@/shared/api/invalidate";

/** A plugin's values at one scope, with its schema. */
export function useScopedSettings(plugin: string, scope: PluginSettingsScope, id?: string) {
	return usePluginSettings(
		plugin,
		{ scope, id },
		{ query: { enabled: scope === "server" || Boolean(id), retry: false } },
	);
}

/** The values in effect for a project (or the server's, with none). */
export function useEffectiveSettings(plugin: string, project?: string) {
	return useEffectivePluginSettings(
		plugin,
		{ project },
		{ query: { enabled: Boolean(plugin), retry: false } },
	);
}

/** The values in effect on a machine, or for a project: what a scope inherits. */
export function useInherited(plugin: string, at: { machine?: string; project?: string }) {
	return useEffectivePluginSettings(plugin, at, { query: { retry: false } });
}

/** Saves values at a scope; every settings query asks again. */
export function useSaveSettings() {
	const refresh = useInvalidate("/settings");
	return useSavePluginSettings({ mutation: { onSettled: refresh } });
}

/** A plugin's values at the server's scope, read now. */
export function serverSettings(plugin: string) {
	return pluginSettings(plugin, { scope: "server" });
}
