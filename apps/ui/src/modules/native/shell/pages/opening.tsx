import { useEffect, useRef, useState } from "react";

import { failure, native, type Boot, type Server } from "@/modules/native/shell/api";
import { Button, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { useTranslation } from "@/shared/i18n";
import { hideSplash, showSplash } from "@/shared/splash/splash";

type Trouble =
	| { kind: "unreachable"; message: string }
	| { kind: "unpaired" }
	| { kind: "missing" };

/**
 * Opening a server's web app: the splash, "Connecting to atlas…", a check
 * that it answers and still knows this app, then its page. The web app's
 * own splash continues this one.
 */
export function Opening({ serverId, path }: { serverId: string; path?: string }) {
	const { t } = useTranslation("native");
	const [server, setServer] = useState<Server>();
	const [boot, setBoot] = useState<Boot>();
	const [trouble, setTrouble] = useState<Trouble>();
	const [round, setRound] = useState(0);
	const [starting, setStarting] = useState(false);
	const ran = useRef(-1);

	useEffect(() => {
		if (ran.current === round) return;
		ran.current = round;
		void (async () => {
			try {
				const booted = await native.boot();
				setBoot(booted);
				const found = booted.servers.servers.find((item) => item.id === serverId);
				if (!found) {
					setTrouble({ kind: "missing" });
					return;
				}
				setServer(found);
				showSplash(t("splash.connecting", { name: found.name }));
				const check = await native.checkServer(found.id);
				if (!check.reachable) {
					setTrouble({ kind: "unreachable", message: check.message ?? "" });
				} else if (!check.paired) {
					setTrouble({ kind: "unpaired" });
				} else {
					await native.openServer(found.id, path);
				}
			} catch (error) {
				setTrouble({ kind: "unreachable", message: failure(error).message });
			}
		})();
	}, [serverId, path, round, t]);

	useEffect(() => {
		if (trouble) hideSplash();
	}, [trouble]);
	if (!trouble) return null;

	if (trouble.kind === "missing" || !server) {
		return (
			<Page title={t("screens.noSuchServer")}>
				<Button onClick={() => go("/servers")}>{t("screens.servers")}</Button>
			</Page>
		);
	}
	if (trouble.kind === "unpaired") {
		return (
			<Page
				title={t("opening.unpaired.title", { name: server.name })}
				lead={t("opening.unpaired.lead")}
			>
				<div className="flex gap-2">
					<Button onClick={() => go("/connect", { address: server.url })}>
						{t("screens.pairAgain")}
					</Button>
					<Button variant="outline" onClick={() => go("/servers")}>
						{t("screens.servers")}
					</Button>
				</div>
			</Page>
		);
	}
	const stoppedHere = server.local && boot?.computer.role === "server" && !boot.computer.running;
	return (
		<Page title={t("opening.unreachable.title", { name: server.name })} lead={server.url}>
			{trouble.message ? <Problem>{trouble.message}</Problem> : null}
			{stoppedHere ? (
				<p className="text-muted-foreground text-sm">{t("opening.unreachable.stopped")}</p>
			) : null}
			<div className="flex flex-wrap gap-2">
				{stoppedHere ? (
					<Button
						disabled={starting}
						onClick={() => {
							setStarting(true);
							showSplash(t("splash.starting"));
							void native.startHere().then(
								() => {
									setStarting(false);
									setTrouble(undefined);
									setRound((n) => n + 1);
								},
								(error) => {
									setStarting(false);
									setTrouble({ kind: "unreachable", message: failure(error).message });
								},
							);
						}}
					>
						{t("screens.startHere")}
					</Button>
				) : null}
				<Button
					variant={stoppedHere ? "outline" : "default"}
					onClick={() => {
						setTrouble(undefined);
						setRound((n) => n + 1);
					}}
				>
					{t("screens.tryAgain")}
				</Button>
				<Button variant="ghost" onClick={() => go("/servers")}>
					{t("opening.unreachable.otherServers")}
				</Button>
			</div>
		</Page>
	);
}
