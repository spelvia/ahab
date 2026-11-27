import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";

import { useMachines } from "@/modules/machines";
import { FolderPicker } from "@/modules/projects/files";
import { useCreateProject } from "@/modules/projects/project/api";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { Switch } from "@/shared/ui/switch";

const field =
	"border-input bg-background text-foreground focus-visible:ring-ring mt-1.5 block w-full rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2";

/**
 * A new project: a folder on one of your machines, with an optional name
 * (the folder's otherwise), group and read-only mode. It opens once made.
 */
export function NewProjectDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation(["projects", "common"]);
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="lg">
				<DialogHeader>
					<DialogTitle>{t("newProject.title")}</DialogTitle>
					<DialogDescription>{t("newProject.description")}</DialogDescription>
				</DialogHeader>
				{/* Remounted each time it opens, so it starts empty. */}
				{open ? <NewProjectForm onDone={() => onOpenChange(false)} /> : null}
			</DialogContent>
		</Dialog>
	);
}

function NewProjectForm({ onDone }: { onDone: () => void }) {
	const { t } = useTranslation(["projects", "common"]);
	const create = useCreateProject();
	const navigate = useNavigate();
	const machines = useMachines();
	const [name, setName] = useState("");
	const [group, setGroup] = useState("");
	const [readOnly, setReadOnly] = useState(false);
	const list = (machines.data?.items ?? [])
		.filter((machine) => machine.state === "active")
		.map((machine) => ({ id: machine.id, name: machine.name, online: machine.online }));
	return (
		<DialogBody>
			<div className="grid gap-3 sm:grid-cols-2">
				<label className="text-muted-foreground text-xs font-medium">
					{t("newProject.name")}
					<input
						value={name}
						maxLength={64}
						placeholder={t("newProject.namePlaceholder")}
						onChange={(event) => setName(event.target.value)}
						className={`${field} h-9`}
					/>
				</label>
				<label className="text-muted-foreground text-xs font-medium">
					{t("newProject.group")}
					<input
						value={group}
						maxLength={64}
						placeholder={t("newProject.groupPlaceholder")}
						onChange={(event) => setGroup(event.target.value)}
						className={`${field} h-9`}
					/>
				</label>
			</div>
			<label className="text-muted-foreground flex items-center gap-2 text-xs">
				<Switch label={t("newProject.readOnly")} checked={readOnly} onCheckedChange={setReadOnly} />
				{t("newProject.readOnly")}
			</label>
			{machines.isPending ? (
				<p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>
			) : (
				<FolderPicker
					machines={list}
					action={t("newProject.create")}
					pending={create.isPending}
					error={create.error ? explain(create.error, t("newProject.failed")) : undefined}
					onPick={(machine, path) =>
						create.mutate(
							{
								data: {
									machine,
									path,
									name: name.trim() || null,
									group: group.trim() || null,
									mode: readOnly ? "readOnly" : "readWrite",
								},
							},
							{
								onSuccess: (project) => {
									onDone();
									void navigate({ to: "/p/$project", params: { project: project.id } });
								},
							},
						)
					}
				/>
			)}
		</DialogBody>
	);
}
