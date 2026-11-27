import { Link, Outlet } from "@tanstack/react-router";
import {
	ArrowLeft,
	Bell,
	Blocks,
	Bot,
	FileCode,
	FolderKanban,
	Info,
	Keyboard,
	LayoutGrid,
	Monitor,
	Palette,
	MonitorSmartphone,
	Puzzle,
} from "lucide-react";
import type { ReactNode } from "react";

import { WaitingCount } from "@/modules/machines";
import { useDesktopApp } from "@/shared/desktop/hooks";
import { useTranslation } from "@/shared/i18n";

const item =
	"flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm font-medium whitespace-nowrap transition-colors duration-150 [&_svg]:size-4";
const activeItem = { className: "bg-accent text-heading" };
const inactiveItem = {
	className: "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
};

type SettingsPath =
	| "/settings"
	| "/settings/projects"
	| "/settings/machines"
	| "/settings/clients"
	| "/settings/plugins"
	| "/settings/features"
	| "/settings/notifications"
	| "/settings/editor"
	| "/settings/agent-editor"
	| "/settings/appearance"
	| "/settings/keyboard"
	| "/settings/about";

function Entry({
	to,
	icon,
	label,
	exact,
	extra,
}: {
	to: SettingsPath;
	icon: ReactNode;
	label: string;
	exact?: boolean;
	extra?: ReactNode;
}) {
	return (
		<li>
			<Link
				to={to}
				activeOptions={exact ? { exact: true } : undefined}
				className={item}
				activeProps={activeItem}
				inactiveProps={inactiveItem}
			>
				{icon}
				{label}
				{extra}
			</Link>
		</li>
	);
}

/** A group's heading: what its settings belong to. */
function Group({ label, children }: { label: string; children: ReactNode }) {
	return (
		<li className="contents md:block">
			<p className="text-muted-foreground hidden px-2.5 pt-4 pb-1 text-[0.6875rem] font-semibold tracking-wide uppercase md:block">
				{label}
			</p>
			<ul className="contents md:flex md:flex-col md:gap-1">{children}</ul>
		</li>
	);
}

/**
 * Settings: a slim bar back to the work, its sections in a side list (a row
 * of tabs on narrow screens). The sections are grouped by where they are
 * kept: Ahab's own (the server's, the same on every client), and this
 * client's (this browser's, or the app's).
 */
export function SettingsLayout() {
	const { t } = useTranslation("shell");
	const app = useDesktopApp();
	const here = !app
		? t("settings.groups.browser")
		: app.platform === "android"
			? t("settings.groups.phone")
			: t("settings.groups.app");
	return (
		<div className="bg-background flex min-h-svh flex-col">
			<header className="bg-muted/40 border-border flex h-10 shrink-0 items-center gap-2 border-b px-2">
				<Link
					to="/"
					className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring flex h-7 items-center gap-1.5 rounded-md px-2 text-sm outline-none focus-visible:ring-2"
				>
					<ArrowLeft className="size-4" />
					{t("settings.back")}
				</Link>
				<span className="text-heading ml-2 text-sm font-semibold">{t("settings.title")}</span>
			</header>
			<div className="mx-auto grid w-full max-w-6xl flex-1 grid-cols-1 content-start gap-6 px-4 py-8 md:grid-cols-[12rem_1fr] md:gap-10">
				<nav aria-label={t("settings.title")} className="min-w-0 md:pt-1">
					<ul className="border-border flex [scrollbar-width:none] gap-1 overflow-x-auto border-b pb-2 md:flex-col md:overflow-visible md:border-b-0 md:pb-0">
						<Entry
							to="/settings"
							exact
							icon={<LayoutGrid />}
							label={t("settings.sections.overview")}
						/>
						<Group label={t("settings.groups.ahab")}>
							<Entry
								to="/settings/projects"
								icon={<FolderKanban />}
								label={t("settings.sections.projects")}
							/>
							<Entry
								to="/settings/machines"
								icon={<Monitor />}
								label={t("settings.sections.machines")}
								extra={<WaitingCount />}
							/>
							<Entry
								to="/settings/clients"
								icon={<MonitorSmartphone />}
								label={t("settings.sections.clients")}
							/>
							<Entry
								to="/settings/plugins"
								icon={<Puzzle />}
								label={t("settings.sections.plugins")}
							/>
							<Entry
								to="/settings/features"
								icon={<Blocks />}
								label={t("settings.sections.features")}
							/>
							<Entry
								to="/settings/notifications"
								icon={<Bell />}
								label={t("settings.sections.notifications")}
							/>
							<Entry
								to="/settings/editor"
								icon={<FileCode />}
								label={t("settings.sections.editor")}
							/>
							<Entry
								to="/settings/agent-editor"
								icon={<Bot />}
								label={t("settings.sections.agentEditor")}
							/>
						</Group>
						<Group label={here}>
							<Entry
								to="/settings/appearance"
								icon={<Palette />}
								label={t("settings.sections.appearance")}
							/>
							<Entry
								to="/settings/keyboard"
								icon={<Keyboard />}
								label={t("settings.sections.keyboard")}
							/>
						</Group>
						<li className="md:pt-4">
							<ul className="contents">
								<Entry to="/settings/about" icon={<Info />} label={t("settings.sections.about")} />
							</ul>
						</li>
					</ul>
				</nav>
				<div className="min-w-0">
					<Outlet />
				</div>
			</div>
		</div>
	);
}
