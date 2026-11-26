import { Link } from "@tanstack/react-router";

import { useMachines } from "@/modules/machines";
import { useServerVersion } from "@/modules/platform";
import { useDesktopApp } from "@/shared/desktop/hooks";
import { useTranslation } from "@/shared/i18n";

/**
 * Settings → About: the Ahab each part runs. The server (with its
 * web app and its own daemon), this app when there is one, and each machine,
 * marked when it is behind the server. Updating from here comes with
 * `ahab update`.
 */
export function SettingsAbout() {
	const { t } = useTranslation("shell");
	const server = useServerVersion();
	const app = useDesktopApp();
	const machines = useMachines();
	const active = (machines.data?.items ?? []).filter((machine) => machine.state === "active");
	return (
		<section className="space-y-8">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("settings.about.title")}</h1>
				<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
					{t("settings.about.intro")}
				</p>
			</header>
			<dl className="border-border divide-border divide-y border-y text-sm">
				<Row label={t("settings.about.server")} value={server} />
				{app ? (
					<Row
						label={
							app.platform === "android"
								? t("settings.about.phoneApp")
								: t("settings.about.desktopApp")
						}
						value={app.version}
					/>
				) : null}
			</dl>
			<section className="space-y-3">
				<h2 className="text-heading text-base font-semibold">{t("settings.about.machines")}</h2>
				{active.length === 0 ? (
					<p className="text-muted-foreground text-sm">{t("settings.about.noMachines")}</p>
				) : (
					<dl className="border-border divide-border divide-y border-y text-sm">
						{active.map((machine) => (
							<Row
								key={machine.id}
								label={
									<Link
										to="/settings/machines/$id"
										params={{ id: machine.id }}
										className="hover:underline"
									>
										{machine.name}
									</Link>
								}
								value={machine.version ?? undefined}
								note={machine.outdated ? t("settings.about.behind") : undefined}
							/>
						))}
					</dl>
				)}
			</section>
		</section>
	);
}

function Row({ label, value, note }: { label: React.ReactNode; value?: string; note?: string }) {
	const { t } = useTranslation("shell");
	return (
		<div className="flex items-baseline gap-4 py-2.5">
			<dt className="w-40 shrink-0 font-medium">{label}</dt>
			<dd className="flex min-w-0 items-baseline gap-2">
				<span className="font-mono">{value ?? t("settings.about.unknown")}</span>
				{note ? <span className="text-warning text-xs font-medium">{note}</span> : null}
			</dd>
		</div>
	);
}
