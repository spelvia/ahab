import { Link, useNavigate } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, ChevronRight, FolderOpen, Plus } from "lucide-react";
import { useState } from "react";

import {
	useArchivedProjects,
	useOpenFolder,
	useProjects,
	useSetProjectOrder,
} from "@/modules/projects/project/api";
import { NewProjectDialog } from "@/modules/projects/project/components/new-project-dialog";
import { OpenFolderDialog } from "@/modules/projects/project/components/open-folder-dialog";
import { ProjectMark } from "@/modules/projects/project/components/project-switcher";
import { groupProjects, projectPlace } from "@/modules/projects/project/model";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * Settings → Projects: every project under its group's heading, each
 * opening its own page (name, group, folder, editor and language settings,
 * archive and delete), and the ways to start one: a folder on a machine.
 */
export function ProjectsPage() {
	const { t } = useTranslation(["projects", "common"]);
	const active = useProjects();
	// Projects are listed in this order everywhere: the switcher, Alt+1…9.
	const order = useSetProjectOrder();
	const archived = useArchivedProjects();
	const [creating, setCreating] = useState(false);
	const [opening, setOpening] = useState(false);
	const open = useOpenFolder();
	const navigate = useNavigate();
	return (
		<section className="max-w-3xl space-y-8">
			<header className="flex flex-wrap items-end gap-3">
				<div className="min-w-0 flex-1 space-y-1">
					<h1 className="font-heading text-heading text-2xl font-semibold">
						{t("projectsPage.title")}
					</h1>
					<p className="text-muted-foreground text-sm">{t("projectsPage.description")}</p>
				</div>
				<div className="flex gap-2">
					<Button variant="outline" size="sm" onClick={() => setOpening(true)}>
						<FolderOpen />
						{t("projectsPage.openFolder")}
					</Button>
					<Button size="sm" onClick={() => setCreating(true)}>
						<Plus />
						{t("projectsPage.newProject")}
					</Button>
				</div>
			</header>

			<List
				projects={active.data?.items}
				empty={t("projectsPage.empty")}
				onOrder={(ids) => order.mutate({ data: { ids } })}
			/>

			{archived.data && archived.data.items.length > 0 ? (
				<section className="space-y-3">
					<h2 className="text-heading text-base font-bold">{t("projectsPage.archived")}</h2>
					<List projects={archived.data.items} />
				</section>
			) : null}

			<NewProjectDialog open={creating} onOpenChange={setCreating} />
			<OpenFolderDialog
				open={opening}
				onOpenChange={(next) => {
					setOpening(next);
					open.reset();
				}}
				title={t("projectsPage.openFolderDialog.title")}
				description={t("projectsPage.openFolderDialog.description")}
				action={t("projectsPage.openFolderDialog.action")}
				pending={open.isPending}
				error={
					open.error ? explain(open.error, t("projectsPage.openFolderDialog.failed")) : undefined
				}
				onPick={(machine, path) =>
					open.mutate(
						{ data: { machine, path } },
						{
							onSuccess: (opened) => {
								setOpening(false);
								void navigate({
									to: "/settings/projects/$id",
									params: { id: opened.project.id },
								});
							},
						},
					)
				}
			/>
		</section>
	);
}

function List({
	projects,
	empty,
	onOrder,
}: {
	projects?: Project[];
	empty?: string;
	/** Moves a project up or down: the new order of every project's ID. */
	onOrder?: (ids: string[]) => void;
}) {
	const { t } = useTranslation(["projects", "common"]);
	if (!projects)
		return <p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>;
	if (projects.length === 0 && empty)
		return (
			<p className="border-border text-muted-foreground rounded-lg border px-4 py-3 text-sm">
				{empty}
			</p>
		);
	const groups = groupProjects(projects);
	return (
		<div className="space-y-4">
			{groups.map(({ group, projects: members }) => (
				<section
					key={group ?? ""}
					className="space-y-1.5"
					aria-label={group ?? t("projectsPage.ungrouped")}
				>
					{group !== null || groups.length > 1 ? (
						<h3 className="text-muted-foreground text-xs font-medium">
							{group ?? t("projectsPage.ungrouped")}
						</h3>
					) : null}
					<ul className="border-border divide-border divide-y rounded-lg border">
						{members.map((project, at) => (
							<li key={project.id} className="group flex items-center">
								<Link
									to="/settings/projects/$id"
									params={{ id: project.id }}
									className="hover:bg-accent/50 focus-visible:ring-ring flex min-w-0 flex-1 items-center gap-3 px-4 py-3 outline-none focus-visible:ring-2 focus-visible:ring-inset"
								>
									<ProjectMark name={project.name} icon={project.icon} />
									<div className="min-w-0 flex-1">
										<p className="text-heading truncate text-sm font-medium">{project.name}</p>
										<p className="text-muted-foreground truncate text-xs" title={project.path}>
											{projectPlace(project)}
										</p>
									</div>
									<ChevronRight className="text-muted-foreground size-4 shrink-0" />
								</Link>
								{onOrder ? (
									<span className="flex shrink-0 gap-0.5 pr-2">
										{([-1, 1] as const).map((step) => {
											const neighbour = members[at + step];
											const Icon = step < 0 ? ArrowUp : ArrowDown;
											const label =
												step < 0
													? t("projectsPage.moveUp", { name: project.name })
													: t("projectsPage.moveDown", { name: project.name });
											return (
												<button
													key={step}
													type="button"
													disabled={!neighbour}
													onClick={() => {
														if (!neighbour) return;
														// Within its group: swapped with its neighbour there.
														const ids = projects.map((item) =>
															item.id === project.id
																? neighbour.id
																: item.id === neighbour.id
																	? project.id
																	: item.id,
														);
														onOrder(ids);
													}}
													aria-label={label}
													title={label}
													className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2 disabled:invisible"
												>
													<Icon className="size-3.5" />
												</button>
											);
										})}
									</span>
								) : null}
							</li>
						))}
					</ul>
				</section>
			))}
		</div>
	);
}
