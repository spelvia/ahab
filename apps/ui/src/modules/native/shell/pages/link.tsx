import { useEffect, useRef, useState } from "react";

import { failure, native, type Link, type Server } from "@/modules/native/shell/api";
import { Button, Card, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { Trans, useTranslation } from "@/shared/i18n";
import { hideSplash, showSplash } from "@/shared/splash/splash";

/**
 * An `ahab://` link: it only ever pairs, joins or opens a page,
 * and pairing and joining wait for the person to confirm what the link
 * asks, with the server's address and fingerprint shown.
 */
export function LinkScreen({ url }: { url?: string }) {
	const { t } = useTranslation(["native", "common"]);
	const [link, setLink] = useState<Link>();
	const [servers, setServers] = useState<Server[]>([]);
	const [problem, setProblem] = useState<string>();
	const read = useRef(false);

	useEffect(() => {
		if (read.current) return;
		read.current = true;
		void (async () => {
			try {
				const text = url ?? (await native.takeLink());
				if (!text) {
					go("/start");
					return;
				}
				const [parsed, boot] = await Promise.all([native.parseLink(text), native.boot()]);
				setServers(boot.servers.servers);
				const known = boot.servers.servers.find((server) => server.url === parsed.server);
				if (parsed.kind === "open" && known) {
					showSplash(t("splash.connecting", { name: known.name }));
					await native.openServer(known.id, parsed.path);
					return;
				}
				setLink(parsed);
			} catch (error) {
				setProblem(failure(error).message);
			}
		})();
	}, [url, t]);

	if (problem && !link) {
		return (
			<Page title={t("link.broken")}>
				<Problem>{problem}</Problem>
				<Button variant="outline" onClick={() => go("/start")}>
					{t("screens.back")}
				</Button>
			</Page>
		);
	}
	if (!link) return null;

	const act = async (work: () => Promise<Server | undefined>, status: string) => {
		setProblem(undefined);
		showSplash(status);
		try {
			const server = await work();
			if (server) await native.openServer(server.id);
		} catch (error) {
			setProblem(failure(error).message);
			hideSplash();
		}
	};

	const address = <code className="text-heading font-mono text-sm break-all">{link.server}</code>;
	if (link.kind === "pair") {
		return (
			<Page title={t("link.pair.title")} lead={t("link.pair.lead")}>
				<Card>{address}</Card>
				<p className="text-muted-foreground text-sm">{t("link.pair.warning")}</p>
				{problem ? <Problem>{problem}</Problem> : null}
				<div className="flex gap-2">
					<Button
						onClick={() =>
							void act(() => native.pair(link.server, link.secret, link.code), t("splash.pairing"))
						}
					>
						{t("link.pair.action")}
					</Button>
					<Button variant="outline" onClick={() => go("/start")}>
						{t("common:actions.cancel")}
					</Button>
				</div>
			</Page>
		);
	}
	if (link.kind === "join") {
		const paired = servers.find((server) => server.url === link.server);
		return (
			<Page title={t("link.join.title")} lead={t("link.join.lead")}>
				<Card>
					{address}
					<p className="text-muted-foreground text-xs">
						{link.fingerprint ? (
							<Trans
								t={t}
								i18nKey="link.join.fingerprint"
								values={{ fingerprint: link.fingerprint }}
								components={{ code: <code className="font-mono" /> }}
							/>
						) : (
							t("link.join.noFingerprint")
						)}
					</p>
				</Card>
				{problem ? <Problem>{problem}</Problem> : null}
				<div className="flex gap-2">
					<Button
						onClick={() =>
							void act(async () => {
								await native.joinWithCode(link.server, link.code, link.fingerprint);
								return paired;
							}, t("splash.adding"))
						}
					>
						{t("screens.addThisComputer")}
					</Button>
					<Button variant="outline" onClick={() => go("/start")}>
						{t("common:actions.cancel")}
					</Button>
				</div>
			</Page>
		);
	}
	return (
		<Page title={t("link.unknown.title")} lead={link.server}>
			<Button onClick={() => go("/connect", { address: link.server })}>
				{t("link.unknown.action")}
			</Button>
		</Page>
	);
}
