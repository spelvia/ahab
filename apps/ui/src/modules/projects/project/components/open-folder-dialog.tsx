import { useMachines } from "@/modules/machines";
import { FolderPicker } from "@/modules/projects/files";
import { useTranslation } from "@/shared/i18n";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";

/** A folder picker in a dialog: "Open folder…" (its project, or a new one). */
export function OpenFolderDialog({
	open,
	onOpenChange,
	title,
	description,
	action,
	pending,
	error,
	onPick,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description: string;
	action: string;
	pending?: boolean;
	error?: string;
	onPick: (machine: string, path: string) => void;
}) {
	const { t } = useTranslation(["projects", "common"]);
	const machines = useMachines();
	const list = (machines.data?.items ?? [])
		.filter((machine) => machine.state === "active")
		.map((machine) => ({ id: machine.id, name: machine.name, online: machine.online }));
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="lg">
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription>{description}</DialogDescription>
				</DialogHeader>
				<DialogBody>
					{machines.isPending ? (
						<p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>
					) : (
						<FolderPicker
							machines={list}
							action={action}
							pending={pending}
							error={error}
							onPick={onPick}
						/>
					)}
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}
