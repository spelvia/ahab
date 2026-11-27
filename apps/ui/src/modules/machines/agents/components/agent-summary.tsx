import { cn } from "cn";

import { useAgents } from "@/modules/machines/agents/api";
import { AgentIcon } from "@/modules/machines/agents/components/agent-icon";
import { stateOf } from "@/modules/machines/agents/model";
import { useTranslation } from "@/shared/i18n";

const dot: Record<string, string> = {
	ready: "bg-success",
	notSignedIn: "bg-warning",
	unsupportedVersion: "bg-warning",
	error: "bg-destructive",
};

/**
 * A machine's agents at a glance: each installed one's mark with a dot for
 * its state (green ready, amber needs you, red cannot tell). Agents that
 * are not installed are left out.
 */
export function AgentSummary({ machineId }: { machineId: string }) {
	const { t } = useTranslation("machines");
	const agents = useAgents(machineId);
	const installed = (agents.data?.items ?? []).filter((agent) => agent.state !== "notInstalled");
	if (!agents.data) return <span className="text-muted-foreground">…</span>;
	if (installed.length === 0)
		return <span className="text-muted-foreground">{t("agentSummary.none")}</span>;
	return (
		<ul className="flex items-center gap-2.5">
			{installed.map((agent) => (
				<li
					key={agent.agent}
					className="relative"
					title={t("agentSummary.agentState", {
						agent: agent.label,
						state: stateOf(agent.state).word,
					})}
				>
					<AgentIcon agent={agent.agent} />
					<span
						className={cn(
							"ring-background absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full ring-2",
							dot[agent.state] ?? "bg-muted-foreground",
						)}
					/>
					<span className="sr-only">
						{t("agentSummary.agentState", {
							agent: agent.label,
							state: stateOf(agent.state).word,
						})}
					</span>
				</li>
			))}
		</ul>
	);
}
