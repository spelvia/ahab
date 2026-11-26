import { ArrowLeft, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import {
	failure,
	native,
	type Boot,
	type Settings,
	type ThisComputer,
	type Updates,
} from "@/modules/native/shell/api";
import { Button, Card, Confirm, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { t as translate, useTranslation } from "@/shared/i18n";
import { Switch } from "@/shared/ui/switch";

/** In words: what this computer does. */
function describeRole(computer: ThisComputer): string {
	const ns = "native";
	if (computer.role === "server") {
		if (!computer.running) return translate("settings.role.stopped", { ns });
		return computer.url
			? translate("settings.role.runningAt", { ns, url: computer.url })
			: translate("settings.role.running", { ns });
	}
	if (computer.role === "machine") {
		if (!computer.joined) {
			return computer.running
				? translate("settings.role.machineElsewhere", { ns })
				: translate("settings.role.machineElsewhereStopped", { ns });
		}
		return computer.running
			? translate("settings.role.machine", { ns, server: computer.joined })
			: translate("settings.role.machineStopped", { ns, server: computer.joined });
	}
	return translate("settings.role.none", { ns });
}

type Asking = "stop" | "leave" | undefined;

/**
 * App settings: This computer (its role, and the changes only these
 * screens make) and the app's own settings.
 */
export function SettingsScreen({ confirmStop }: { confirmStop: boolean }) {
	const { t } = useTranslation(["native", "common"]);
	const [boot, setBoot] = useState<Boot>();
	const [problem, setProblem] = useState<string>();
	const [busy, setBusy] = useState<string>();
	const [asking, setAsking] = useState<Asking>(confirmStop ? "stop" : undefined);
	const [shortcut, setShortcut] = useState("");
	const [updates, setUpdates] = useState<Updates>();

	const load = useCallback(() => {
		void native.boot().then(
			(loaded) => {
				setBoot(loaded);
				setShortcut(loaded.settings.globalShortcut ?? "");
			},
			(error) => setProblem(failure(error).message),
		);
		void native.checkForUpdates().then(setUpdates, () => undefined);
	}, []);
	useEffect(load, [load]);

	const change = async (label: string, work: () => Promise<unknown>) => {
		setProblem(undefined);
		setBusy(label);
		setAsking(undefined);
		try {
			await work();
		} catch (error) {
			setProblem(failure(error).message);
		} finally {
			setBusy(undefined);
			load();
		}
	};
	const save = (next: Partial<Settings>) => {
		if (!boot) return;
		void change("settings", () => native.setSettings({ ...boot.settings, ...next }));
	};

	if (!boot)
		return problem ? <Page title={t("settings.title")}>{<Problem>{problem}</Problem>}</Page> : null;
	const { computer, settings, servers } = boot;
	const remote = servers.servers.filter((server) => !server.local);
	const keepInTray = settings.keepInTray ?? computer.running;
	// What runs here, as the texts about starting and stopping it name it.
	const context = computer.role === "machine" ? "daemon" : undefined;

	return (
		<Page title={t("settings.title")} wide>
			<Button variant="ghost" size="sm" className="-mt-3 -ml-2" onClick={() => go("/servers")}>
				<ArrowLeft />
				{t("screens.servers")}
			</Button>
			{problem ? <Problem>{problem}</Problem> : null}

			<section aria-labelledby="computer-heading" className="space-y-3">
				<h2 id="computer-heading" className="font-heading text-heading text-lg font-semibold">
					{t("settings.computer.heading")}
				</h2>
				<Card>
					<p className="text-sm">{describeRole(computer)}</p>
					<dl className="text-muted-foreground grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1 text-xs">
						<dt>{t("settings.computer.installed")}</dt>
						<dd className="font-mono break-all">
							{computer.installed
								? `${computer.installed.path} (${computer.installed.version ?? "?"})`
								: t("settings.computer.nothing")}
						</dd>
						<dt>{t("settings.computer.bundled")}</dt>
						<dd className="font-mono">
							{computer.bundled?.version ?? t("settings.computer.nothing")}
						</dd>
						<dt>{t("settings.computer.service")}</dt>
						<dd>
							{computer.serviceManager
								? computer.managed
									? t("settings.computer.managedService", { manager: computer.serviceManager })
									: computer.serviceManager
								: t("settings.computer.noServiceManager")}
						</dd>
						{computer.machine?.name ? (
							<>
								<dt>{t("settings.computer.machine")}</dt>
								<dd>
									{computer.machine.name} ({computer.machine.state})
								</dd>
							</>
						) : null}
					</dl>
					{busy && busy !== "settings" ? (
						<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
							<LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" />
							{busy}
						</p>
					) : computer.managed ? (
						<p className="text-muted-foreground text-sm">{t("settings.computer.managed")}</p>
					) : asking === "stop" ? (
						<Confirm
							question={t("settings.computer.stop.question", { context })}
							detail={t("settings.computer.stop.detail")}
							yes={t("settings.computer.stop.yes")}
							destructive
							onYes={() =>
								void change(t("settings.computer.stop.busy", { context }), native.stopHere)
							}
							onNo={() => setAsking(undefined)}
						/>
					) : asking === "leave" ? (
						<Confirm
							question={
								computer.joined
									? t("settings.computer.leave.question", { server: computer.joined })
									: t("settings.computer.leave.questionUnknown")
							}
							detail={t("settings.computer.leave.detail")}
							yes={t("settings.computer.leave.yes")}
							destructive
							onYes={() => void change(t("settings.computer.leave.busy"), native.leave)}
							onNo={() => setAsking(undefined)}
						/>
					) : (
						<div className="flex flex-wrap gap-2">
							{computer.role === "none" ? (
								<Button size="sm" onClick={() => go("/running", { action: "run" })}>
									{t("settings.computer.runHere")}
								</Button>
							) : computer.running ? (
								<Button size="sm" variant="outline" onClick={() => setAsking("stop")}>
									{t("settings.computer.stop.action", { context })}
								</Button>
							) : (
								<Button
									size="sm"
									onClick={() =>
										void change(t("settings.computer.start.busy", { context }), native.startHere)
									}
								>
									{t("settings.computer.start.action", { context })}
								</Button>
							)}
							{computer.role === "machine" ? (
								<Button size="sm" variant="outline" onClick={() => setAsking("leave")}>
									{t("settings.computer.leave.action", { server: computer.joined ?? "" })}
								</Button>
							) : (
								remote.map((server) => (
									<Button
										key={server.id}
										size="sm"
										variant="ghost"
										onClick={() => go("/join", { server: server.id })}
									>
										{t("settings.computer.addAsMachine", { name: server.name })}
									</Button>
								))
							)}
						</div>
					)}
				</Card>
			</section>

			<section aria-labelledby="app-heading" className="space-y-3">
				<h2 id="app-heading" className="font-heading text-heading text-lg font-semibold">
					{t("settings.app.heading")}
				</h2>
				<Card>
					<Row title={t("settings.app.tray.title")} detail={t("settings.app.tray.detail")}>
						<Switch
							label={t("settings.app.tray.label")}
							checked={keepInTray}
							onCheckedChange={(checked) => save({ keepInTray: checked })}
						/>
					</Row>
					<Row
						title={t("settings.app.notifications.title")}
						detail={t("settings.app.notifications.detail")}
					>
						<Switch
							label={t("settings.app.notifications.title")}
							checked={settings.notifications}
							onCheckedChange={(checked) => save({ notifications: checked })}
						/>
					</Row>
					<Row title={t("settings.app.updates")} detail={updates?.note}>
						<Switch
							label={t("settings.app.updates")}
							checked={settings.checkForUpdates}
							onCheckedChange={(checked) => save({ checkForUpdates: checked })}
						/>
					</Row>
					<form
						className="space-y-1.5"
						onSubmit={(event) => {
							event.preventDefault();
							save({ globalShortcut: shortcut.trim() || null });
						}}
					>
						<label htmlFor="shortcut" className="text-sm font-medium">
							{t("settings.app.shortcut.label")}
						</label>
						<div className="flex gap-2">
							<input
								id="shortcut"
								value={shortcut}
								onChange={(event) => setShortcut(event.target.value)}
								placeholder={t("settings.app.shortcut.placeholder")}
								className="border-input bg-background h-8 min-w-0 flex-1 rounded-md border px-2 font-mono text-sm"
							/>
							<Button size="sm" variant="outline" type="submit">
								{t("common:actions.save")}
							</Button>
						</div>
					</form>
				</Card>
				<p className="text-muted-foreground text-xs text-pretty">
					{servers.store === "keychain"
						? t("settings.app.keychain")
						: (servers.storeNote ?? t("settings.app.encryptedFile"))}{" "}
					{t("settings.app.version", { version: boot.version })}
				</p>
				<Button variant="ghost" size="sm" onClick={() => void native.quit()}>
					{t("settings.app.quit")}
				</Button>
			</section>
		</Page>
	);
}

function Row({
	title,
	detail,
	children,
}: {
	title: string;
	detail?: string;
	children: React.ReactNode;
}) {
	return (
		<div className="flex items-start justify-between gap-4">
			<div className="min-w-0">
				<p className="text-sm font-medium">{title}</p>
				{detail ? <p className="text-muted-foreground text-xs text-pretty">{detail}</p> : null}
			</div>
			{children}
		</div>
	);
}
