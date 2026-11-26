import {
	Box,
	Container,
	Cpu,
	GitBranch,
	Hexagon,
	Languages,
	Shield,
	SquareTerminal,
} from "lucide-react";

import {
	MachineSection,
	PlannedRow,
	SectionRows,
} from "@/modules/machines/machine/components/machine-section";
import { useTranslation } from "@/shared/i18n";

/** What a machine offers sessions, terminals and plugins: its tools and sandboxes. Planned. */
export function MachineEnvironment() {
	const { t } = useTranslation("machines");
	return (
		<div className="space-y-10">
			<MachineSection
				id="tools"
				title={t("environment.tools.title")}
				planned
				description={t("environment.tools.description")}
			>
				<SectionRows>
					<PlannedRow icon={<GitBranch />} title="Git" detail={t("environment.tools.git")} />
					<PlannedRow
						icon={<SquareTerminal />}
						title={t("environment.tools.shell")}
						detail={t("environment.tools.shellDetail")}
					/>
					<PlannedRow icon={<Hexagon />} title="Node.js" detail={t("environment.tools.node")} />
					<PlannedRow
						icon={<Languages />}
						title={t("environment.tools.languageTools")}
						detail={t("environment.tools.languageToolsDetail")}
					/>
				</SectionRows>
			</MachineSection>

			<MachineSection
				id="sandboxes"
				title={t("environment.sandboxes.title")}
				planned
				description={t("environment.sandboxes.description")}
			>
				<SectionRows>
					<PlannedRow
						icon={<Shield />}
						title={t("environment.sandboxes.os")}
						detail={t("environment.sandboxes.osDetail")}
					/>
					<PlannedRow
						icon={<Container />}
						title="Docker"
						detail={t("environment.sandboxes.docker")}
					/>
					<PlannedRow
						icon={<Box />}
						title="Kubernetes"
						detail={t("environment.sandboxes.kubernetes")}
					/>
					<PlannedRow
						icon={<Cpu />}
						title={t("environment.sandboxes.vm")}
						detail={t("environment.sandboxes.vmDetail")}
						source={t("environment.sandboxes.later")}
					/>
				</SectionRows>
			</MachineSection>
		</div>
	);
}
