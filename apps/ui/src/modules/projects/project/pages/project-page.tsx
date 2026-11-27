import { useNavigate } from "@tanstack/react-router";
import { cn } from "cn";
import { Folder } from "lucide-react";
import { type ReactNode, useState } from "react";

import { SettingsFields } from "@/modules/platform";
import { useDeleteProject, useProject, useUpdateProject } from "@/modules/projects/project/api";
import { ProjectIconField } from "@/modules/projects/project/components/project-icon-field";
import { projectState } from "@/modules/projects/project/model";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { Switch } from "@/shared/ui/switch";

/**
 * Settings → Projects → a project → Settings: its name, description and
 * group, its folder and whether it is read-only, its own editor settings,
 * and archiving or deleting it. Files on the machine are never touched.
 * Its header and tabs are {@link ProjectLayout}'s.
 */
export function ProjectPage({ projectId }: { projectId: string }) {
	const project = useProject(projectId);
	return project.data ? <Details project={project.data} /> : null;
}

function Details({ project }: { project: Project }) {
	const { t } = useTranslation(["projects", "common"]);
	const archived = project.state === "archived";
	return (
		<div className="space-y-10">
			<General project={project} />
			<FolderSection project={project} />
			{archived ? null : (
				<Section
					title={t("projectPage.editor.title")}
					description={t("projectPage.editor.description", { name: project.name })}
				>
					<SettingsFields section="editor" project={project.id} />
				</Section>
			)}
			<Lifecycle project={project} />
		</div>
	);
}

function Section({
	title,
	description,
	action,
	children,
}: {
	title: string;
	description?: string;
	action?: ReactNode;
	children: ReactNode;
}) {
	return (
		<section className="space-y-3">
			<div className="flex items-end gap-3">
				<div className="min-w-0 flex-1 space-y-0.5">
					<h2 className="text-heading text-base font-bold">{title}</h2>
					{description ? <p className="text-muted-foreground text-sm">{description}</p> : null}
				</div>
				{action}
			</div>
			{children}
		</section>
	);
}

const field =
	"border-input bg-background focus-visible:ring-ring w-full rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2";

function General({ project }: { project: Project }) {
	const { t } = useTranslation(["projects", "common"]);
	const update = useUpdateProject();
	const [name, setName] = useState(project.name);
	const [description, setDescription] = useState(project.description ?? "");
	const [group, setGroup] = useState(project.group ?? "");
	const changed =
		name.trim() !== project.name ||
		description.trim() !== (project.description ?? "") ||
		group.trim() !== (project.group ?? "");
	return (
		<Section title={t("projectPage.general.title")}>
			<div className="border-border mb-3 rounded-lg border p-4">
				<ProjectIconField project={project} />
			</div>
			<form
				className="border-border space-y-4 rounded-lg border p-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (!changed || !name.trim()) return;
					update.mutate({
						project: project.id,
						data: {
							name: name.trim(),
							description: description.trim(),
							group: group.trim(),
						},
					});
				}}
			>
				<label className="block space-y-1.5">
					<span className="text-muted-foreground text-xs font-medium">
						{t("projectPage.general.name")}
					</span>
					<input
						value={name}
						maxLength={60}
						onChange={(event) => setName(event.target.value)}
						className={cn(field, "h-9")}
					/>
				</label>
				<label className="block space-y-1.5">
					<span className="text-muted-foreground text-xs font-medium">
						{t("projectPage.general.description")}
					</span>
					<textarea
						value={description}
						rows={2}
						maxLength={500}
						placeholder={t("projectPage.general.descriptionPlaceholder")}
						onChange={(event) => setDescription(event.target.value)}
						className={cn(field, "resize-y py-2")}
					/>
				</label>
				<label className="block space-y-1.5">
					<span className="text-muted-foreground text-xs font-medium">
						{t("projectPage.general.group")}
					</span>
					<input
						value={group}
						maxLength={64}
						placeholder={t("projectPage.general.groupPlaceholder")}
						onChange={(event) => setGroup(event.target.value)}
						className={cn(field, "h-9")}
					/>
				</label>
				{update.isError ? (
					<p role="alert" className="text-destructive text-sm">
						{update.error.message}
					</p>
				) : null}
				<div className="flex justify-end gap-2">
					{changed ? (
						<Button
							type="button"
							variant="ghost"
							onClick={() => {
								setName(project.name);
								setDescription(project.description ?? "");
								setGroup(project.group ?? "");
							}}
						>
							{t("common:actions.undo")}
						</Button>
					) : null}
					<Button type="submit" disabled={!changed || !name.trim() || update.isPending}>
						{update.isPending ? t("common:actions.saving") : t("common:actions.save")}
					</Button>
				</div>
			</form>
		</Section>
	);
}

/** Its folder on its machine, and whether agents and tools may change it. */
function FolderSection({ project }: { project: Project }) {
	const { t } = useTranslation(["projects", "common"]);
	const edit = useUpdateProject();
	const state = projectState(project);
	return (
		<Section
			title={t("projectPage.folder.title")}
			description={t("projectPage.folder.description")}
		>
			<div className="border-border flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border px-4 py-3">
				<Folder className="text-muted-foreground size-4 shrink-0" />
				<div className="min-w-0 flex-1">
					<p className="text-heading truncate font-mono text-sm" title={project.path}>
						{project.path}
					</p>
					<p className="text-muted-foreground flex items-center gap-1 text-xs">
						<span className={cn("size-1.5 rounded-full", state.dot)} />
						{state.label}
					</p>
				</div>
				<label className="text-muted-foreground flex items-center gap-2 text-xs">
					{t("projectPage.folder.readOnly")}
					<Switch
						label={t("projectPage.folder.readOnlyLabel", { name: project.name })}
						checked={project.mode === "readOnly"}
						disabled={edit.isPending}
						onCheckedChange={(readOnly) =>
							edit.mutate({
								project: project.id,
								data: { mode: readOnly ? "readOnly" : "readWrite" },
							})
						}
					/>
				</label>
			</div>
			{edit.isError ? (
				<p role="alert" className="text-destructive text-sm">
					{edit.error.message}
				</p>
			) : null}
		</Section>
	);
}

function Lifecycle({ project }: { project: Project }) {
	const { t } = useTranslation(["projects", "common"]);
	const update = useUpdateProject();
	const remove = useDeleteProject();
	const navigate = useNavigate();
	const [confirm, setConfirm] = useState(false);
	const archived = project.state === "archived";
	return (
		<Section title={t("projectPage.lifecycle.title")}>
			<ul className="border-border divide-border divide-y rounded-lg border">
				<li className="flex flex-wrap items-center gap-4 px-4 py-3">
					<div className="min-w-0 flex-1">
						<p className="text-heading text-sm font-medium">
							{archived
								? t("projectPage.lifecycle.restoreTitle")
								: t("projectPage.lifecycle.archiveTitle")}
						</p>
						<p className="text-muted-foreground text-sm">
							{archived
								? t("projectPage.lifecycle.restoreDescription")
								: t("projectPage.lifecycle.archiveDescription")}
						</p>
					</div>
					<Button
						size="sm"
						variant="outline"
						disabled={update.isPending}
						onClick={() =>
							update.mutate({
								project: project.id,
								data: { state: archived ? "active" : "archived" },
							})
						}
					>
						{archived ? t("projectPage.lifecycle.restore") : t("projectPage.lifecycle.archive")}
					</Button>
				</li>
				<li className="flex flex-wrap items-center gap-4 px-4 py-3">
					<div className="min-w-0 flex-1">
						<p className="text-heading text-sm font-medium">
							{t("projectPage.lifecycle.deleteTitle")}
						</p>
						<p className="text-muted-foreground text-sm">
							{t("projectPage.lifecycle.deleteDescription")}
						</p>
					</div>
					<Button size="sm" variant="destructive" onClick={() => setConfirm(true)}>
						{t("common:actions.delete")}
					</Button>
				</li>
			</ul>
			<ConfirmDialog
				open={confirm}
				onOpenChange={(open) => {
					setConfirm(open);
					remove.reset();
				}}
				title={t("projectPage.deleteProject.title", { name: project.name })}
				description={t("projectPage.deleteProject.description")}
				action={t("projectPage.deleteProject.action")}
				destructive
				pending={remove.isPending}
				error={remove.error?.message}
				onConfirm={() =>
					remove.mutate(
						{ project: project.id },
						{ onSuccess: () => void navigate({ to: "/settings/projects" }) },
					)
				}
			/>
		</Section>
	);
}
