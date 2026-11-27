import { Link } from "@tanstack/react-router";
import { LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { isPaired } from "@/modules/clients/client";
import { usePair } from "@/modules/clients/pairing/api";
import { markPaired } from "@/shared/api/pairing";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import { SplashDone } from "@/shared/splash/splash-done";
import { Button } from "@/shared/ui/button";

/** Only a path on this server: never another site. */
export function safeNext(next: string | undefined): string {
	return next?.startsWith("/") && !next.startsWith("//") && !next.startsWith("/\\") ? next : "/";
}

function replaceLocation(path: string) {
	globalThis.location.replace(path);
}

/**
 * A pairing link, `/pair#<secret>`: the secret after `#` never reaches the
 * server's logs; this page posts it, then opens Ahab (`?next=` says where).
 * A browser that is paired already just goes on.
 */
export function PairPage({
	next,
	go = replaceLocation,
}: {
	next?: string;
	go?: (path: string) => void;
}) {
	const { t } = useTranslation("clients");
	const pair = usePair();
	const [secret] = useState(() => globalThis.location?.hash.replace(/^#/, "") ?? "");
	const tried = useRef(false);
	const target = safeNext(next);

	useEffect(() => {
		if (tried.current) return;
		tried.current = true;
		// The secret leaves the address bar and the history.
		globalThis.history?.replaceState(
			null,
			"",
			globalThis.location.pathname + globalThis.location.search,
		);
		void (async () => {
			if (await isPaired()) {
				go(target);
				return;
			}
			if (!secret) return;
			pair.mutate(
				{ data: { secret, kind: "browser" } },
				{
					onSuccess: () => {
						markPaired();
						go(target);
					},
				},
			);
		})();
	}, [pair, secret, target, go]);

	return (
		<main className="bg-background flex min-h-svh items-start justify-center px-4 pt-[12vh]">
			<section className="w-full max-w-md space-y-4">
				<h1 className="font-heading text-heading text-2xl font-semibold">{t("pairPage.title")}</h1>
				{pair.isError || !secret ? (
					<>
						{/* Pairing keeps the splash; only a failure takes over from it. */}
						<SplashDone />
						<p role="alert" className="text-destructive text-sm">
							{secret
								? explain(pair.error, t("pairPage.problems.failed"), {
										pairing_expired: t("pairPage.problems.expired"),
										pairing_invalid: t("pairPage.problems.invalid"),
										too_many_attempts: t("pairing.problems.tooManyAttempts"),
									})
								: t("pairPage.problems.noSecret")}
						</p>
						<Button asChild variant="outline">
							<Link to="/">{t("pairPage.another")}</Link>
						</Button>
					</>
				) : (
					<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
						<LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" />
						{t("pairPage.pairing")}
					</p>
				)}
			</section>
		</main>
	);
}
