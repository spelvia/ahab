import { Settings2 } from "lucide-react";

import {
	MachineSection,
	PlannedRow,
	SectionRows,
} from "@/modules/machines/machine/components/machine-section";
import { useTranslation } from "@/shared/i18n";

/** What is set for one machine alone. Planned. */
export function MachineSettings() {
	const { t } = useTranslation("machines");
	return (
		<div className="space-y-10">
			<MachineSection
				id="settings"
				title={t("machineSettings.title")}
				planned
				description={t("machineSettings.description")}
			>
				<SectionRows>
					<PlannedRow
						icon={<Settings2 />}
						title={t("machineSettings.pluginSettings")}
						detail={t("machineSettings.pluginSettingsDetail")}
					/>
					<PlannedRow
						icon={<Settings2 />}
						title={t("machineSettings.pluginsHere")}
						detail={t("machineSettings.pluginsHereDetail")}
					/>
					<PlannedRow
						icon={<Settings2 />}
						title={t("machineSettings.daemon")}
						detail={t("machineSettings.daemonDetail")}
					/>
					<PlannedRow
						icon={<Settings2 />}
						title={t("machineSettings.sandbox")}
						detail={t("machineSettings.sandboxDetail")}
					/>
				</SectionRows>
			</MachineSection>
		</div>
	);
}
