import { useEffect, useRef } from "react";

import { useInvalidate } from "@/shared/api/invalidate";

const NAMES = [
	"pairing.requested",
	"pairing.answered",
	"pairing.used",
	"client.paired",
	"client.renamed",
	"client.revoked",
];

/** A pairing link was used: which, and the client it paired. */
export interface PairingUsed {
	pairing: string;
	client: string;
	name: string;
}

/**
 * Keeps requests and clients current as the server announces changes on
 * its event stream, and tells `onUsed` when a pairing link was used.
 */
export function useClientEvents(enabled: boolean, onUsed?: (used: PairingUsed) => void) {
	const refresh = useInvalidate("/pairing-requests", "/clients", "/pairings/");
	const latest = useRef({ refresh, onUsed });
	useEffect(() => {
		latest.current = { refresh, onUsed };
	});
	useEffect(() => {
		if (!enabled || typeof EventSource === "undefined") return;
		const source = new EventSource(`/api/v1/plugin-events?names=${NAMES.join(",")}`);
		source.addEventListener("message", (message) => {
			const event = JSON.parse(String(message.data)) as { name: string; data: unknown };
			latest.current.refresh();
			if (event.name === "pairing.used") latest.current.onUsed?.(event.data as PairingUsed);
		});
		return () => source.close();
	}, [enabled]);
}
