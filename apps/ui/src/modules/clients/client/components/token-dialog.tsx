import { useState } from "react";

import { useCreateToken } from "@/modules/clients/client/api";
import { Trans, useTranslation } from "@/shared/i18n";
import { CopyButton } from "@/shared/ui/copy-button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { NameDialog } from "@/shared/ui/name-dialog";

/** Create token: asks what it is for, then shows the token, once. */
export function TokenDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("clients");
	const create = useCreateToken();
	const [token, setToken] = useState<string>();
	if (token) {
		return (
			<Dialog
				open={open}
				onOpenChange={(next) => {
					if (!next) setToken(undefined);
					onOpenChange(next);
				}}
			>
				<DialogContent size="md">
					<DialogHeader>
						<DialogTitle>{t("token.title")}</DialogTitle>
						<DialogDescription>
							<Trans
								t={t}
								i18nKey="token.description"
								values={{ header: "Authorization: Bearer …", variable: "AHAB_TOKEN" }}
								components={{ code: <code className="font-mono text-xs" /> }}
							/>
						</DialogDescription>
					</DialogHeader>
					<DialogBody>
						<div className="bg-background border-border flex items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-3">
							<code className="text-heading flex-1 font-mono text-xs break-all">{token}</code>
							<CopyButton text={token} label={t("token.copyLabel")} />
						</div>
					</DialogBody>
				</DialogContent>
			</Dialog>
		);
	}
	return (
		<NameDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("token.create.title")}
			label={t("token.create.label")}
			action={t("token.create.action")}
			pending={create.isPending}
			error={create.error?.message}
			onSubmit={(name) =>
				create.mutate({ data: { name } }, { onSuccess: (created) => setToken(created.token) })
			}
		/>
	);
}
