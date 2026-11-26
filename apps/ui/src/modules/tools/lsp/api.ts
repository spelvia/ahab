import { useQueryClient } from "@tanstack/react-query";

import {
	getListLanguageServersQueryKey,
	useLanguageServerLog as useGeneratedLanguageServerLog,
	useListLanguageServers,
	restartLanguageServer,
	useRestartLanguageServer as useGeneratedRestartLanguageServer,
} from "@/shared/api/generated/tools-lsp/tools-lsp";

/** The language servers running on a machine, asked again while shown. */
export function useLanguageServers(machineId: string, enabled = true) {
	return useListLanguageServers(machineId, {
		query: { enabled, retry: false, refetchInterval: 10_000 },
	});
}

/** Restarts a language server on a machine, then shows the list again. */
export function useRestartLanguageServer(machineId: string) {
	const queryClient = useQueryClient();
	return useGeneratedRestartLanguageServer({
		mutation: {
			onSettled: () =>
				queryClient.invalidateQueries({ queryKey: getListLanguageServersQueryKey(machineId) }),
		},
	});
}

/** A language server's log, asked again every few seconds while it is open. */
export function useLanguageServerLog(machineId: string, server: string, enabled = true) {
	return useGeneratedLanguageServerLog(machineId, server, {
		query: { enabled, retry: false, refetchInterval: 3_000 },
	});
}

/** Restarts a language server (the file window's status item). */
export function restartServer(machineId: string, server: string) {
	return restartLanguageServer(machineId, server);
}

/** Where a project's LSPs settings are. */
export function projectToolsPath(projectId: string): string {
	return `/settings/projects/${projectId}/lsps`;
}
