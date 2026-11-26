import { AppWindow, Pencil, Plus, Settings, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { failure, native, type Overview, type Server } from "@/modules/native/shell/api";
import { Button, Card, Confirm, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { useTranslation } from "@/shared/i18n";
import { showSplash } from "@/shared/splash/splash";

/** The servers this app is paired with: open, rename, remove, add. */
export function Servers() {
	const { t } = useTranslation(["native", "common"]);
	const [overview, setOverview] = useState<Overview>();
	const [problem, setProblem] = useState<string>();
	const [removing, setRemoving] = useState<Server>();
	const [renaming, setRenaming] = useState<{ id: string; name: string }>();

	const load = useCallback(() => {
		void native.boot().then(
			(boot) => setOverview(boot.servers),
			(error) => setProblem(failure(error).message),
		);
	}, []);
	useEffect(load, [load]);

	const run = async (work: () => Promise<unknown>) => {
		setProblem(undefined);
		try {
			await work();
			load();
		} catch (error) {
			setProblem(failure(error).message);
		}
	};

	const servers = overview?.servers ?? [];
	return (
		<Page
			title={t("servers.title")}
			lead={
				overview?.store === "encryptedFile"
					? t("servers.leadEncryptedFile")
					: t("servers.leadKeychain")
			}
		>
			{problem ? <Problem>{problem}</Problem> : null}
			{overview && servers.length === 0 ? (
				<p className="text-muted-foreground text-sm">{t("servers.none")}</p>
			) : null}
			<ul className="space-y-2">
				{servers.map((server) => (
					<li key={server.id}>
						<Card className="space-y-2">
							{renaming?.id === server.id ? (
								<form
									className="flex gap-2"
									onSubmit={(event) => {
										event.preventDefault();
										void run(() => native.renameServer(server.id, renaming.name)).then(() =>
											setRenaming(undefined),
										);
									}}
								>
									<input
										aria-label={t("servers.name")}
										autoFocus
										value={renaming.name}
										onChange={(event) => setRenaming({ id: server.id, name: event.target.value })}
										className="border-input bg-background h-8 min-w-0 flex-1 rounded-md border px-2 text-sm"
									/>
									<Button size="sm" type="submit">
										{t("common:actions.save")}
									</Button>
								</form>
							) : (
								<div className="flex items-start justify-between gap-3">
									<div className="min-w-0">
										<p className="text-heading truncate font-semibold">
											{server.name}
											{server.local && server.name !== "This computer" ? (
												<span className="text-muted-foreground ml-2 text-xs font-normal">
													{t("servers.local")}
												</span>
											) : null}
										</p>
										<p className="text-muted-foreground truncate font-mono text-xs">{server.url}</p>
									</div>
									<Button
										size="sm"
										onClick={() => {
											showSplash(t("splash.connecting", { name: server.name }));
											go("/opening", { server: server.id });
										}}
									>
										{t("servers.open")}
									</Button>
								</div>
							)}
							{removing?.id === server.id ? (
								<Confirm
									question={t("servers.remove.question", { name: server.name })}
									detail={t("servers.remove.detail")}
									yes={t("common:actions.remove")}
									destructive
									onYes={() =>
										void run(() => native.removeServer(server.id)).then(() =>
											setRemoving(undefined),
										)
									}
									onNo={() => setRemoving(undefined)}
								/>
							) : (
								<div className="flex flex-wrap gap-1">
									<Button
										size="xs"
										variant="ghost"
										onClick={() => void run(() => native.openServerWindow(server.id))}
									>
										<AppWindow />
										{t("servers.newWindow")}
									</Button>
									<Button
										size="xs"
										variant="ghost"
										onClick={() => setRenaming({ id: server.id, name: server.name })}
									>
										<Pencil />
										{t("servers.rename")}
									</Button>
									<Button size="xs" variant="ghost" onClick={() => setRemoving(server)}>
										<Trash2 />
										{t("common:actions.remove")}
									</Button>
								</div>
							)}
						</Card>
					</li>
				))}
			</ul>
			<div className="flex gap-2">
				<Button variant="outline" onClick={() => go("/connect")}>
					<Plus />
					{t("servers.add")}
				</Button>
				<Button variant="ghost" onClick={() => go("/settings")}>
					<Settings />
					{t("servers.settings")}
				</Button>
			</div>
		</Page>
	);
}
