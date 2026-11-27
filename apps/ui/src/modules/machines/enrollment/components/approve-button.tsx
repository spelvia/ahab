import { useApproveMachine } from "@/modules/machines/enrollment/api";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** Approves a machine that waits to join. Compare its check code first. */
export function ApproveButton({ id, name }: { id: string; name: string }) {
	const { t } = useTranslation("machines");
	const approve = useApproveMachine();
	return (
		<>
			<Button
				size="sm"
				disabled={approve.isPending}
				aria-label={t("approve.label", { name })}
				onClick={() => approve.mutate({ id })}
			>
				{approve.isPending ? t("approve.approving") : t("approve.approve")}
			</Button>
			{approve.error && (
				<span className="text-destructive text-xs" role="alert">
					{approve.error.message}
				</span>
			)}
		</>
	);
}
