import { Bot, FolderPlus, Plus, SquareTerminal } from "lucide-react";
import { useState } from "react";

import { useCreateFolder } from "@/modules/sessions/session/api";
import { useTranslation } from "@/shared/i18n";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";

/** The Sessions panel's +: an agent, a terminal, or a folder to group them. */
export function SessionsAddMenu({
	project,
	onNewAgent,
	onNewTerminal,
}: {
	project: string;
	onNewAgent: () => void;
	/** None while the Terminal feature is off. */
	onNewTerminal?: () => void;
}) {
	const { t } = useTranslation("sessions");
	const create = useCreateFolder();
	const [naming, setNaming] = useState(false);
	return (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger
					aria-label={t("addMenu.label")}
					title={onNewTerminal ? t("addMenu.titleWithTerminal") : t("addMenu.title")}
					className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring data-[state=open]:bg-accent grid size-8 place-items-center rounded-md outline-none focus-visible:ring-2 md:size-6"
				>
					<Plus className="size-4" />
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<DropdownMenuItem onSelect={onNewAgent}>
						<Bot />
						{t("addMenu.newAgent")}
					</DropdownMenuItem>
					{onNewTerminal ? (
						<DropdownMenuItem onSelect={onNewTerminal}>
							<SquareTerminal />
							{t("addMenu.newTerminal")}
						</DropdownMenuItem>
					) : null}
					<DropdownMenuSeparator />
					<DropdownMenuItem onSelect={() => setNaming(true)}>
						<FolderPlus />
						{t("addMenu.newFolder")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
			<NameDialog
				open={naming}
				onOpenChange={setNaming}
				title={t("folderDialog.newTitle")}
				label={t("folderDialog.name")}
				action={t("folderDialog.create")}
				pending={create.isPending}
				error={create.error?.message}
				onSubmit={(name) =>
					create.mutate({ project, data: { name } }, { onSuccess: () => setNaming(false) })
				}
			/>
		</>
	);
}
