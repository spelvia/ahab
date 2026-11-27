import { Link } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

import { useClients } from "@/modules/clients";
import { useMachines } from "@/modules/machines";
import { useServerVersion } from "@/modules/platform";
import { usePlugins } from "@/modules/plugins";
import { useProjects } from "@/modules/projects";
import { useTranslation } from "@/shared/i18n";

/**
 * Settings → Overview, where Settings opens: one line per part of Ahab, each
 * saying what needs attention, each opening its page. Nothing is changed here.
 */
export function SettingsOverview() {
	const { t } = useTranslation("shell");
	const version = useServerVersion();
	const machines = useMachines().data?.items ?? [];
	const projects = useProjects().data?.items ?? [];
	const plugins = usePlugins().data?.items ?? [];
	const clients = useClients().data?.items ?? [];
	const unused = clients.filter((client) => client.unused).length;
	const active = machines.filter((machine) => machine.state === "active");
	const waiting = machines.length - active.length;
	const offline = active.filter((machine) => !machine.online).length;
	const behind = active.filter((machine) => machine.outdated).length;
	return (
		<section className="space-y-8">
			<header className="space-y-1">
				<h1 className="text-2xl font-semibold">{t("settings.overview.title")}</h1>
			</header>
			<ul className="border-border divide-border divide-y rounded-lg border">
				<Line to="/settings/about" label={t("settings.overview.ahab")}>
					{version ? t("settings.overview.version", { version }) : null}
				</Line>
				<Line to="/settings/projects" label={t("settings.sections.projects")}>
					{t("settings.overview.projects", { count: projects.length })}
				</Line>
				<Line to="/settings/machines" label={t("settings.sections.machines")}>
					{t("settings.overview.machines", { count: active.length })}
					{offline ? (
						<Attention>{t("settings.overview.offline", { count: offline })}</Attention>
					) : null}
					{behind ? (
						<Attention>{t("settings.overview.behind", { count: behind })}</Attention>
					) : null}
					{waiting ? (
						<Attention>{t("settings.overview.waiting", { count: waiting })}</Attention>
					) : null}
				</Line>
				<Line to="/settings/clients" label={t("settings.sections.clients")}>
					{t("settings.overview.clients", { count: clients.length })}
					{unused ? (
						<Attention>{t("settings.overview.unused", { count: unused })}</Attention>
					) : null}
				</Line>
				<Line to="/settings/plugins" label={t("settings.sections.plugins")}>
					{t("settings.overview.plugins", {
						count: plugins.length,
						enabled: plugins.filter((plugin) => plugin.enabled).length,
					})}
				</Line>
			</ul>
		</section>
	);
}

function Line({
	to,
	label,
	children,
}: {
	to:
		| "/settings/about"
		| "/settings/projects"
		| "/settings/machines"
		| "/settings/clients"
		| "/settings/plugins";
	label: string;
	children: ReactNode;
}) {
	return (
		<li>
			<Link
				to={to}
				className="hover:bg-accent/50 focus-visible:ring-ring flex items-center gap-4 px-4 py-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-inset"
			>
				<span className="w-28 shrink-0 font-medium">{label}</span>
				<span className="text-muted-foreground flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2">
					{children}
				</span>
				<ChevronRight className="text-muted-foreground size-4 shrink-0" />
			</Link>
		</li>
	);
}

function Attention({ children }: { children: ReactNode }) {
	return <span className="text-warning font-medium">· {children}</span>;
}
