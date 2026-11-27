import { Navigate, useNavigate } from "@tanstack/react-router";

import { useMachines } from "@/modules/machines";
import { FolderPicker } from "@/modules/projects/files";
import { useOpenFolder, useProjects } from "@/modules/projects/project/api";
import { useTranslation } from "@/shared/i18n";
import { lastProject, rememberProject } from "@/shared/preferences/store";

/**
 * Opening Ahab: back to the last project, or on first run one question,
 * which folder to work in.
 */
export function StartPage() {
	const { t } = useTranslation(["projects", "common"]);
	const projects = useProjects();
	const machines = useMachines();
	const openFolder = useOpenFolder();
	const navigate = useNavigate();

	if (projects.isPending) return null;
	const items = projects.data?.items ?? [];
	const target = items.find((project) => project.id === lastProject()) ?? items[0];
	if (target) {
		return <Navigate to="/p/$project" params={{ project: target.id }} replace />;
	}

	const list = (machines.data?.items ?? [])
		.filter((machine) => machine.state === "active")
		.map((machine) => ({ id: machine.id, name: machine.name, online: machine.online }));

	return (
		<main className="bg-background flex min-h-svh items-start justify-center px-4 pt-[12vh]">
			<section className="w-full max-w-xl">
				<img src="/ahab-mark.svg" alt="" className="h-7 w-auto dark:hidden" />
				<img src="/ahab-mark-white.svg" alt="" className="hidden h-7 w-auto dark:block" />
				<h1 className="font-heading text-heading mt-6 text-2xl font-semibold">
					{t("startPage.title")}
				</h1>
				<p className="text-muted-foreground mt-2 mb-6">{t("startPage.description")}</p>
				{machines.isPending ? (
					<p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>
				) : (
					<FolderPicker
						machines={list}
						action={t("startPage.open")}
						pending={openFolder.isPending}
						error={openFolder.error?.message}
						onPick={(machine, path) =>
							openFolder.mutate(
								{ data: { machine, path } },
								{
									onSuccess: (opened) => {
										rememberProject(opened.project.id);
										void navigate({
											to: "/p/$project",
											params: { project: opened.project.id },
										});
									},
								},
							)
						}
					/>
				)}
			</section>
		</main>
	);
}
