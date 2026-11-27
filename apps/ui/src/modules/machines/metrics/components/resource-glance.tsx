import { Link } from "@tanstack/react-router";

import { useMachineMetricsNowAndThen } from "@/modules/machines/metrics/api";
import { share } from "@/modules/machines/metrics/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { GlanceItem } from "@/shared/ui/glance";

/** A machine's CPU, memory and fullest disk, for its overview. */
export function ResourceGlance({ machine }: { machine: Machine }) {
	const { t } = useTranslation("machines");
	const metrics = useMachineMetricsNowAndThen(machine.id, machine.online);
	const data = metrics.data;
	const fullest = data?.disks
		.map((disk) => ({ mount: disk.mount, used: share(disk.total - disk.available, disk.total) }))
		.toSorted((a, b) => b.used - a.used)[0];
	return (
		<GlanceItem
			label={t("resourceGlance.label")}
			value={
				!machine.online
					? t("resourceGlance.offline")
					: data
						? t("resourceGlance.cpu", { percent: Math.round(data.cpu.usagePercent) })
						: "…"
			}
			detail={
				data
					? [
							t("resourceGlance.memory", {
								percent: Math.round(share(data.memory.used, data.memory.total)),
							}),
							fullest &&
								t("resourceGlance.disk", {
									mount: fullest.mount,
									percent: Math.round(fullest.used),
								}),
						]
							.filter(Boolean)
							.join(" · ")
					: undefined
			}
			link={
				<Link
					to="/settings/machines/$id/resources"
					params={{ id: machine.id }}
					className="text-muted-foreground hover:text-foreground underline underline-offset-4"
				>
					{t("resourceGlance.link")}
				</Link>
			}
		/>
	);
}
