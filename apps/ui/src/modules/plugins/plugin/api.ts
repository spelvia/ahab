import { useQuery } from "@tanstack/react-query";

import {
	getListPluginsQueryKey,
	useGetPlugin,
	useChangePlugin,
	useDisablePlugin,
	useEnablePlugin,
	useInstallPlugin,
	useListPlugins,
	usePluginLogs,
	usePluginStates,
	useRemovePlugin,
	useUpdatePlugin,
} from "@/shared/api/generated/plugins-plugin/plugins-plugin";
import { useInvalidate } from "@/shared/api/invalidate";

/** The installed plugins, in the order their interceptors run. */
export function usePlugins() {
	return useListPlugins({ query: { refetchInterval: 10_000 } });
}

/** How each plugin is doing on the machines online. */
export function useRunStates() {
	return usePluginStates({ query: { refetchInterval: 3_000 } });
}

/** A plugin's logs on a machine, asked again while shown. */
export function useLogs(plugin: string, machine: string | undefined) {
	return usePluginLogs(
		plugin,
		{ machine: machine ?? "" },
		{ query: { enabled: Boolean(machine), refetchInterval: 3_000, retry: false } },
	);
}

/** Every change to plugins, each refreshing the list and the states. */
export function usePluginActions() {
	const refresh = useInvalidate("/plugins");
	const options = { mutation: { onSettled: refresh } };
	return {
		install: useInstallPlugin(options),
		enable: useEnablePlugin(options),
		disable: useDisablePlugin(options),
		update: useUpdatePlugin(options),
		remove: useRemovePlugin(options),
		change: useChangePlugin(options),
	};
}

/** The list's query key, to ask for it again. */
export const pluginsQueryKey = getListPluginsQueryKey;

/** One plugin. */
export function usePlugin(id: string) {
	return useGetPlugin(id, { query: { refetchInterval: 10_000, retry: false } });
}

/**
 * One of a plugin's text files as installed (`OVERVIEW.md`, `README.md`,
 * `CHANGELOG.md`): its text, or `null` when the plugin has none.
 */
export function usePluginText(id: string, path: string, digest: string | undefined) {
	return useQuery({
		queryKey: ["plugin-file", id, path, digest],
		enabled: Boolean(digest),
		staleTime: Number.POSITIVE_INFINITY,
		queryFn: async ({ signal }) => {
			const response = await fetch(`/api/v1/plugins/${id}/files/${path}?v=${digest}`, { signal });
			return response.ok ? response.text() : null;
		},
	});
}
