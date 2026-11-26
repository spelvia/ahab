import { ShieldAlert, ShieldCheck } from "lucide-react";

import { permissionModes } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { Picker } from "@/shared/ui/picker";

/** What the agent may do without asking: Plan, Ask, Accept edits, Bypass… */
export function ModePicker({
	agent,
	native,
	value,
	disabled,
	onChange,
}: {
	agent: string | undefined;
	native?: { available: { id: string; name: string; description?: string | null }[] };
	value: string | undefined;
	disabled?: boolean;
	onChange: (mode: string) => void;
}) {
	const { t } = useTranslation("sessions");
	const modes = permissionModes(agent);
	if (native?.available.length)
		return (
			<Picker
				label={t("permissionModes.label")}
				icon={<ShieldCheck />}
				value={value}
				disabled={disabled}
				options={native.available.map((mode) => ({
					value: mode.id,
					label: mode.name,
					description: mode.description ?? undefined,
				}))}
				onChange={onChange}
			/>
		);
	if (modes.length === 0) return null;
	const risky = modes.find((mode) => mode.id === value)?.risky;
	return (
		<Picker
			label={t("permissionModes.label")}
			icon={risky ? <ShieldAlert className="text-warning" /> : <ShieldCheck />}
			value={value}
			disabled={disabled}
			options={modes.map((mode) => ({
				value: mode.id,
				label: t(`permissionModes.${mode.id}.label`),
				description: t(`permissionModes.${mode.id}.description`),
				icon: mode.risky ? <ShieldAlert className="text-warning" /> : <ShieldCheck />,
			}))}
			onChange={onChange}
		/>
	);
}
