import { Link } from "@tanstack/react-router";

import { useAgents, useAgentUpdates } from "@/modules/machines/agents/api";
import { formatList, useTranslation } from "@/shared/i18n";
import { GlanceItem } from "@/shared/ui/glance";

/** How many of a machine's agents are ready, for its overview. */
export function AgentGlance({ machineId }: { machineId: string }) {
	const { t } = useTranslation("machines");
	const agents = useAgents(machineId);
	const installed = (agents.data?.items ?? []).filter((agent) => agent.state !== "notInstalled");
	const ready = installed.filter((agent) => agent.state === "ready");
	return (
		<GlanceItem
			label={t("agentGlance.label")}
			value={
				agents.data
					? t("agentGlance.value", { ready: ready.length, installed: installed.length })
					: "…"
			}
			detail={
				formatList(
					ready.map((agent) => agent.label),
					"unit",
				) || t("agentGlance.noneReady")
			}
			link={
				<Link
					to="/settings/machines/$id/agents"
					params={{ id: machineId }}
					className="text-muted-foreground hover:text-foreground underline underline-offset-4"
				>
					{t("agentGlance.link")}
				</Link>
			}
		/>
	);
}

/** Which of a machine's agents have a newer version out, for its overview. */
export function UpdateGlance({ machineId }: { machineId: string }) {
	const { t } = useTranslation("machines");
	const agents = useAgents(machineId);
	const installed = (agents.data?.items ?? []).some((agent) => agent.state !== "notInstalled");
	const updates = useAgentUpdates(machineId, installed);
	const newer = (updates.data?.items ?? []).filter((update) => update.available);
	return (
		<GlanceItem
			label={t("updateGlance.label")}
			value={
				updates.data
					? newer.length > 0
						? t("updateGlance.out", { count: newer.length })
						: t("updateGlance.none")
					: "…"
			}
			detail={
				newer.length > 0
					? formatList(
							newer.map((update) => `${update.label} ${update.latest}`),
							"unit",
						)
					: updates.data
						? t("updateGlance.upToDate")
						: undefined
			}
			link={
				<Link
					to="/settings/machines/$id/agents"
					params={{ id: machineId }}
					search={{ agent: undefined }}
					className="text-muted-foreground hover:text-foreground underline underline-offset-4"
				>
					{t("updateGlance.link")}
				</Link>
			}
		/>
	);
}
