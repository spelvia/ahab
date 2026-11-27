import { RotateCw } from "lucide-react";

import { useAgentUsageOn, useRefreshAgentUsage } from "@/modules/machines/usage/api";
import { UsageWindows } from "@/modules/machines/usage/components/usage-windows";
import { METERED, summary, type Tone, worst } from "@/modules/machines/usage/model";
import type { AgentUsage } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * An agent's plan usage on a machine, when it has readable limits: the
 * usage and how pressing its nearest limit is. Never asked for agents
 * without plan limits (Pi).
 */
export function usePlanUsage(
	machineId: string,
	agent: string,
): { usage: AgentUsage; tone: Tone } | undefined {
	const metered = METERED.has(agent);
	const query = useAgentUsageOn(machineId, metered);
	const usage = query.data?.items.find((item) => item.agent === agent);
	if (!metered || !usage || usage.status !== "available" || usage.windows.length === 0)
		return undefined;
	return { usage, tone: worst(summary(usage).length ? summary(usage) : usage.windows) };
}

/** Every limit of an agent's plan, with when each starts over, and Check again. */
export function UsagePanel({ machineId, usage }: { machineId: string; usage: AgentUsage }) {
	const { t } = useTranslation("machines");
	const refresh = useRefreshAgentUsage(machineId);
	return (
		<div className="space-y-3">
			<div className="flex items-baseline justify-between gap-3">
				<p className="text-heading text-sm font-semibold">
					{usage.label}
					{usage.plan && <span className="text-muted-foreground font-normal"> · {usage.plan}</span>}
				</p>
				<Button
					variant="ghost"
					size="icon-xs"
					aria-label={t("usage.checkAgain")}
					title={t("usage.checkAgain")}
					disabled={refresh.isPending}
					onClick={() => refresh.mutate()}
				>
					<RotateCw className={refresh.isPending ? "animate-spin" : undefined} />
				</Button>
			</div>
			<UsageWindows usage={usage} />
			<p className="text-muted-foreground text-[0.6875rem]">
				{t("usage.asOf", { when: agoInWords(usage.fetchedAt) })}
			</p>
			{refresh.isError && (
				<p role="alert" className="text-destructive text-xs">
					{t("usage.checkFailed", { message: refresh.error.message })}
				</p>
			)}
		</div>
	);
}
