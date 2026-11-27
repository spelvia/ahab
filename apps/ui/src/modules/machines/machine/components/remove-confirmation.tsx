import { useRemoveMachine } from "@/modules/machines/machine/api";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** Asks before removing (or rejecting) a machine; it must join again to come back. */
export function RemoveConfirmation({
	machine,
	onCancel,
	onRemoved,
}: {
	machine: Machine;
	onCancel: () => void;
	onRemoved?: () => void;
}) {
	const remove = useRemoveMachine();
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	const rejecting = machine.state === "pending";
	return (
		<div
			className="flex flex-wrap items-center gap-x-3 gap-y-2"
			role="group"
			aria-label={t(
				rejecting ? "removeConfirmation.rejectLabel" : "removeConfirmation.removeLabel",
			)}
		>
			<span className="text-sm">
				<Trans
					t={t}
					i18nKey={
						rejecting ? "removeConfirmation.rejectQuestion" : "removeConfirmation.removeQuestion"
					}
					values={{ name: machine.name }}
					components={{ name: <span className="text-heading font-semibold" /> }}
				/>
			</span>
			<span className="flex gap-2">
				<Button
					variant="destructive"
					size="sm"
					disabled={remove.isPending}
					onClick={() => remove.mutate({ id: machine.id }, { onSuccess: onRemoved })}
				>
					{rejecting ? t("removeConfirmation.reject") : common("actions.remove")}
				</Button>
				<Button variant="ghost" size="sm" onClick={onCancel} autoFocus>
					{t("removeConfirmation.keep")}
				</Button>
			</span>
			{remove.error && (
				<span className="text-destructive text-xs" role="alert">
					{remove.error.message}
				</span>
			)}
		</div>
	);
}
