import {
	currentClient,
	useCreateToken as useGeneratedCreateToken,
	useCurrentClient as useGeneratedCurrentClient,
	useListClients,
	useRenameClient as useGeneratedRenameClient,
	useRevokeClient as useGeneratedRevokeClient,
} from "@/shared/api/generated/clients-client/clients-client";
import { useInvalidate } from "@/shared/api/invalidate";

/** The clients paired with this server. */
export function useClients() {
	return useListClients();
}

/**
 * The client this browser is, asked once: it answers `401 not_paired` when
 * the browser is not paired, which shows the pairing screen.
 */
export function useCurrentClient() {
	return useGeneratedCurrentClient({
		query: { retry: false, staleTime: Infinity },
		request: { timeoutMs: 10_000 },
	});
}

/** Renames a client, then refreshes the list. */
export function useRenameClient() {
	return useGeneratedRenameClient({ mutation: { onSuccess: useInvalidate("/clients") } });
}

/** Revokes a client, then refreshes the list. */
export function useRevokeClient() {
	return useGeneratedRevokeClient({ mutation: { onSuccess: useInvalidate("/clients") } });
}

/** Creates a token for a script; it is shown once. */
export function useCreateToken() {
	return useGeneratedCreateToken({ mutation: { onSuccess: useInvalidate("/clients") } });
}

/** Whether the server knows this browser already. */
export async function isPaired(): Promise<boolean> {
	try {
		await currentClient();
		return true;
	} catch {
		return false;
	}
}
