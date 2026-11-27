import { Ellipsis, Pencil, ShieldOff } from "lucide-react";
import { useState } from "react";

import { useRenameClient, useRevokeClient } from "@/modules/clients/client/api";
import type { Client } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { NameDialog } from "@/shared/ui/name-dialog";

/** A client's ⋯ menu: Rename, and Revoke (Replace credential for this computer), each asked first. */
export function ClientActions({ client, onRevoked }: { client: Client; onRevoked?: () => void }) {
	const { t } = useTranslation("clients");
	const [dialog, setDialog] = useState<"rename" | "revoke">();
	const rename = useRenameClient();
	const revoke = useRevokeClient();
	const local = client.kind === "local";
	return (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						variant="ghost"
						size="icon-sm"
						aria-label={t("table.actionsFor", { name: client.name })}
					>
						<Ellipsis />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<DropdownMenuItem onSelect={() => setDialog("rename")}>
						<Pencil />
						{t("table.rename")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={() => setDialog("revoke")}>
						<ShieldOff />
						{local ? t("table.replaceCredential") : t("table.revoke")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
			<NameDialog
				open={dialog === "rename"}
				onOpenChange={(open) => setDialog(open ? "rename" : undefined)}
				title={t("rename.title", { name: client.name })}
				label={t("rename.label")}
				initial={client.name}
				action={t("rename.action")}
				pending={rename.isPending}
				error={rename.error?.message}
				onSubmit={(name) =>
					rename.mutate(
						{ id: client.id, data: { name } },
						{ onSuccess: () => setDialog(undefined) },
					)
				}
			/>
			<ConfirmDialog
				open={dialog === "revoke"}
				onOpenChange={(open) => setDialog(open ? "revoke" : undefined)}
				title={local ? t("revoke.titleLocal") : t("revoke.title", { name: client.name })}
				description={
					local
						? t("revoke.descriptionLocal")
						: client.current
							? t("revoke.descriptionCurrent")
							: t("revoke.description")
				}
				action={local ? t("revoke.actionLocal") : t("revoke.action")}
				destructive
				pending={revoke.isPending}
				error={revoke.error?.message}
				onConfirm={() =>
					revoke.mutate(
						{ id: client.id },
						{
							onSuccess: () => {
								setDialog(undefined);
								onRevoked?.();
							},
						},
					)
				}
			/>
		</>
	);
}
