import type { ComponentType } from "react";

import { MachineAgents } from "@/modules/machines/agents";
import { useMachine } from "@/modules/machines/machine/api";
import { MachineSection } from "@/modules/machines/machine/components/machine-section";
import { useTranslation } from "@/shared/i18n";

/** A machine's Agents tab: every agent there, in full. */
export function MachineAgentsPage({
	id,
	Models,
	selected,
	onSelect,
}: {
	id: string;
	Models?: ComponentType<{ machineId: string; agent: string }>;
	/** The agent whose details are open. */
	selected?: string;
	onSelect: (agent: string | undefined) => void;
}) {
	const { t } = useTranslation("machines");
	const { data: machine } = useMachine(id);
	if (!machine) return null;
	return (
		<MachineSection
			id="agents"
			title={t("agentsPage.title")}
			description={t("agentsPage.description")}
		>
			<MachineAgents machine={machine} Models={Models} selected={selected} onSelect={onSelect} />
		</MachineSection>
	);
}
