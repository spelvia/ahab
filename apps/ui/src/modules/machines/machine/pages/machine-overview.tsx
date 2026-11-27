import type { ReactNode } from "react";

import { AgentGlance, UpdateGlance } from "@/modules/machines/agents";
import { useMachine } from "@/modules/machines/machine/api";
import { Field, MachineSection } from "@/modules/machines/machine/components/machine-section";
import { fullTime } from "@/modules/machines/machine/model";
import { ResourceGlance } from "@/modules/machines/metrics";
import { useTranslation } from "@/shared/i18n";
import { CopyButton } from "@/shared/ui/copy-button";

/**
 * A machine at a glance (its agents, their updates, its resources, and
 * what other modules add to `glance`), and what it is.
 */
export function MachineOverview({
	id,
	glance,
}: {
	id: string;
	/** More counts for the glance row, as `<li>`s from other modules. */
	glance?: ReactNode;
}) {
	const { t } = useTranslation("machines");
	const { data: machine } = useMachine(id);
	if (!machine) return null;

	return (
		<div className="space-y-10">
			<MachineSection id="glance" title={t("overview.glance")}>
				<ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
					<AgentGlance machineId={machine.id} />
					<UpdateGlance machineId={machine.id} />
					<ResourceGlance machine={machine} />
					{glance}
				</ul>
			</MachineSection>

			<MachineSection
				id="system"
				title={t("overview.system")}
				description={t("overview.systemDescription")}
			>
				<dl className="border-border divide-border divide-y border-y text-sm">
					<Field label={t("overview.os")}>{machine.os ?? "–"}</Field>
					<Field label={t("overview.architecture")}>{machine.arch ?? "–"}</Field>
					<Field label={t("overview.version")}>
						{machine.version ? <span className="font-mono text-xs">{machine.version}</span> : "–"}
					</Field>
					<Field label={t("overview.joined")}>{fullTime(machine.createdAt)}</Field>
					<Field label={t("overview.approved")}>{fullTime(machine.approvedAt)}</Field>
					<Field label={t("overview.lastSeen")}>{fullTime(machine.lastSeenAt)}</Field>
					<Field label={t("overview.checkCode")}>
						<span className="font-mono">{machine.checkCode}</span>
					</Field>
					<Field label={t("overview.fingerprint")}>
						<Copyable text={machine.fingerprint} label={t("overview.fingerprintCopy")} />
					</Field>
					<Field label={t("overview.machineId")}>
						<Copyable text={machine.id} label={t("overview.machineIdCopy")} />
					</Field>
				</dl>
			</MachineSection>
		</div>
	);
}

function Copyable({ text, label }: { text: string; label: string }) {
	return (
		<span className="flex items-center gap-1">
			<span className="truncate font-mono text-xs" title={text}>
				{text}
			</span>
			<CopyButton text={text} label={label} />
		</span>
	);
}
