import { useQueryClient } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { useCurrentClient } from "@/modules/clients/client";
import { PairingScreen } from "@/modules/clients/pairing/components/pairing-screen";
import { markPaired, usePaired } from "@/shared/api/pairing";
import { useTranslation } from "@/shared/i18n";
import { SplashDone } from "@/shared/splash/splash-done";
import { Button } from "@/shared/ui/button";

/**
 * The app for a paired browser, the pairing screen for any other (and as
 * soon as the server answers `not_paired`, such as after revoking it).
 * `/pair` links pair by themselves.
 */
export function PairingGate({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const paired = usePaired();
	const pathname = useRouterState({ select: (state) => state.location.pathname });
	const current = useCurrentClient();
	const queryClient = useQueryClient();
	// The pair page hides the splash itself, when pairing fails.
	if (pathname === "/pair") return children;
	if (!paired) {
		return (
			<>
				<SplashDone />
				<PairingScreen
					onPaired={() => {
						markPaired();
						void queryClient.resetQueries();
					}}
				/>
			</>
		);
	}
	// A moment, so a browser that is not paired never shows the app: the
	// splash stays until then.
	if (!current.data && (current.isFetched || current.isPaused)) {
		return (
			<section className="mx-auto max-w-lg space-y-4 px-6 pt-24">
				<SplashDone />
				<p role="alert">{current.error?.message ?? t("errors.serverUnreachable")}</p>
				<Button disabled={current.isFetching} onClick={() => void current.refetch()}>
					{t("actions.retry")}
				</Button>
			</section>
		);
	}
	if (current.isPending) return null;
	return (
		<>
			<SplashDone />
			{children}
		</>
	);
}
