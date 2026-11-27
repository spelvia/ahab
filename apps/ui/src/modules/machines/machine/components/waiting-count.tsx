import { useMachines } from "@/modules/machines/machine/api";
import { waiting } from "@/modules/machines/machine/model";
import { useTranslation } from "@/shared/i18n";

/**
 * A small amber count beside Settings while machines wait for approval:
 * the only way machines reach the rest of the app.
 */
export function WaitingCount() {
	const { t } = useTranslation("machines");
	const { data } = useMachines();
	const count = data ? waiting(data.items).length : 0;
	if (count === 0) return null;
	return (
		<span
			className="bg-warning text-background inline-flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[0.6875rem] leading-none font-bold"
			title={t("waitingCount.title", { count })}
		>
			{count}
			<span className="sr-only"> {t("waitingCount.waiting")}</span>
		</span>
	);
}
