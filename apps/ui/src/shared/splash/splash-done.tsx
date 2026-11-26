import { useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";

import { hideSplash } from "@/shared/splash/splash";

/**
 * Rendered with the first real screen (the app, the pairing screen, an
 * error): hides the splash once the router has nothing left to load.
 */
export function SplashDone() {
	const idle = useRouterState({ select: (state) => state.status === "idle" });
	useEffect(() => {
		if (idle) hideSplash();
	}, [idle]);
	return null;
}
