import type { StartTerminal } from "@/shared/api/generated/ahab.schemas";
import {
	startTerminal,
	useGetTerminal,
	useKillTerminal as useGeneratedKillTerminal,
	useRemoveTerminal as useGeneratedRemoveTerminal,
	useRestartTerminal as useGeneratedRestartTerminal,
	useStartSessionTerminal as useGeneratedStartSessionTerminal,
	useStartTerminal as useGeneratedStartTerminal,
	useUpdateTerminal as useGeneratedUpdateTerminal,
} from "@/shared/api/generated/sessions-terminal/sessions-terminal";
import { useInvalidate } from "@/shared/api/invalidate";

/** After a change: the sessions (a terminal is one) and terminals. */
function useRefresh() {
	return useInvalidate("/sessions", "/terminals");
}

/** Starts a terminal (a terminal session) in a project. */
export function useStartTerminal() {
	return useGeneratedStartTerminal({ mutation: { onSuccess: useRefresh() } });
}

/** Ends a terminal's process; it stays, exited. */
export function useKillTerminal() {
	return useGeneratedKillTerminal({ mutation: { onSuccess: useRefresh() } });
}

/** Runs a terminal's shell (or command) again. */
export function useRestartTerminal() {
	return useGeneratedRestartTerminal({ mutation: { onSuccess: useRefresh() } });
}

/** A terminal by its ID, asked again now and then (its state, title). */
export function useTerminal(id: string) {
	return useGetTerminal(id, { query: { refetchInterval: 5_000, retry: false } });
}

/** Starts a terminal outside a component (a plugin asked): its session. */
export function startTerminalIn(project: string, body: StartTerminal) {
	return startTerminal(project, body);
}

/** Starts a terminal in a session, beside its chats and terminals. */
export function useStartTerminalIn() {
	return useGeneratedStartSessionTerminal({ mutation: { onSuccess: useRefresh() } });
}

/** Renames a terminal, or moves it to another session. */
export function useUpdateTerminal() {
	return useGeneratedUpdateTerminal({ mutation: { onSuccess: useRefresh() } });
}

/** Removes an exited terminal from its session. */
export function useRemoveTerminal() {
	return useGeneratedRemoveTerminal({ mutation: { onSuccess: useRefresh() } });
}
