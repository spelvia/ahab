import { useEffect, useRef } from "react";

import { onEvent } from "@/shared/api/events";
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
		if (!enabled) return;
		// On the app's one shared event stream (see `@/shared/api/events`).
		const stops = NAMES.map((name) =>
			onEvent(name, (data) => {
				latest.current.refresh();
				if (name === "pairing.used") latest.current.onUsed?.(data as PairingUsed);
			}),
		);
		return () => {
			for (const stop of stops) stop();
		};
	}, [enabled]);
}
