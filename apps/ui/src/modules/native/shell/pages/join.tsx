import { useEffect, useState } from "react";

import { failure, native, type Boot, type Server } from "@/modules/native/shell/api";
import { Button, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { useTranslation } from "@/shared/i18n";
import { hideSplash, showSplash } from "@/shared/splash/splash";

/**
 * Also add this computer as a machine of a paired server (`ahab setup`,
 * with the server's fingerprint pinned): asked first, saying what changes.
 */
export function Join({ serverId }: { serverId: string }) {
	const { t } = useTranslation("native");
	const [boot, setBoot] = useState<Boot>();
	const [problem, setProblem] = useState<string>();
	useEffect(() => {
		void native.boot().then(setBoot, (error) => setProblem(failure(error).message));
	}, []);
	const server: Server | undefined = boot?.servers.servers.find((item) => item.id === serverId);
	if (!boot)
		return problem ? <Page title={t("join.failed")}>{<Problem>{problem}</Problem>}</Page> : null;
	if (!server) {
		return (
			<Page title={t("screens.noSuchServer")}>
				<Button onClick={() => go("/servers")}>{t("screens.servers")}</Button>
			</Page>
		);
	}
	const open = async () => {
		showSplash(t("splash.connecting", { name: server.name }));
		await native.openServer(server.id);
	};
	const join = async () => {
		setProblem(undefined);
		showSplash(t("splash.addingTo", { name: server.name }));
		try {
			await native.joinServer(server.id);
			await open();
		} catch (error) {
			setProblem(failure(error).message);
			hideSplash();
		}
	};
	const runsServer = boot.computer.role === "server";
	return (
		<Page title={t("join.title", { name: server.name })} lead={t("join.lead")}>
			{runsServer ? (
				<p className="text-warning text-sm text-pretty">
					{t("join.runsServer", { name: server.name })}
				</p>
			) : null}
			{problem ? <Problem>{problem}</Problem> : null}
			<div className="flex gap-2">
				<Button onClick={() => void join()}>{t("screens.addThisComputer")}</Button>
				<Button variant="outline" onClick={() => void open()}>
					{t("join.notNow")}
				</Button>
			</div>
		</Page>
	);
}
