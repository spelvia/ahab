import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { useProject } from "@/modules/projects/project/api";
import { ProjectMark } from "@/modules/projects/project/components/project-switcher";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Breadcrumb, breadcrumbLink, Tabs, tabLink } from "@/shared/ui/detail-page";

/**
 * One project in Settings, framed as a machine is: a breadcrumb back to
 * Projects, its name (Open takes you to it), then its tabs: Overview, and its
 * language tools while it is not archived.
 */
export function ProjectLayout({ projectId, children }: { projectId: string; children: ReactNode }) {
	const { t } = useTranslation(["projects", "common"]);
	const project = useProject(projectId);
	const data = project.data;
	const archived = data?.state === "archived";
	return (
		<section className="max-w-3xl space-y-6">
			<Breadcrumb label={t("projectPage.breadcrumb")}>
				<Link to="/settings/projects" className={breadcrumbLink}>
					{t("projectPage.back")}
				</Link>
				{data?.name ?? t("projectPage.project")}
			</Breadcrumb>
			{project.isPending ? (
				<p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>
			) : project.isError ? (
				<p className="text-muted-foreground text-sm">
					{explain(project.error, t("projectPage.loadFailed"))}
				</p>
			) : data ? (
				<>
					<header className="space-y-4">
						<div className="flex items-center gap-3">
							<ProjectMark name={data.name} icon={data.icon} />
							<h1 className="font-heading text-heading min-w-0 truncate text-2xl font-semibold">
								{data.name}
							</h1>
							{archived ? (
								<span className="border-border text-muted-foreground rounded-full border px-2 py-0.5 text-xs">
									{t("projectPage.archived")}
								</span>
							) : (
								<Button size="sm" variant="outline" className="ml-auto" asChild>
									<Link to="/p/$project" params={{ project: data.name }}>
										{t("projectPage.open")}
									</Link>
								</Button>
							)}
						</div>
						<Tabs label={t("projectPage.tabsLabel", { name: data.name })}>
							<Link
								to="/settings/projects/$id"
								params={{ id: data.id }}
								activeOptions={{ exact: true }}
								{...tabLink}
							>
								{t("projectPage.tabs.overview")}
							</Link>
							<Link to="/settings/projects/$id/settings" params={{ id: data.id }} {...tabLink}>
								{t("projectPage.tabs.settings")}
							</Link>
							{archived ? null : (
								<Link to="/settings/projects/$id/lsps" params={{ id: data.id }} {...tabLink}>
									{t("projectPage.tabs.lsps")}
								</Link>
							)}
						</Tabs>
					</header>
					{children}
				</>
			) : null}
		</section>
	);
}
