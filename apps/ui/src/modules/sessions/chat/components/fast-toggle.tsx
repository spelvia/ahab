import { cn } from "cn";
import { Zap } from "lucide-react";
import { useContext } from "react";

import type { FastMode } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { PickerPlacement } from "@/shared/ui/picker";
import { Switch } from "@/shared/ui/switch";

/**
 * Fast mode (Claude Code's and Codex's faster tier), where the model offers
 * it: a ⚡ that is on or off. Nothing for models without one.
 */
export function FastToggle({
	fast,
	value,
	disabled,
	onChange,
}: {
	/** The model's fast tier; none when it has no such tier. */
	fast: FastMode | null | undefined;
	/** Chosen on or off; undefined follows the provider's default. */
	value: boolean | null | undefined;
	disabled?: boolean;
	onChange: (fast: boolean) => void;
}) {
	const { t } = useTranslation("sessions");
	const placement = useContext(PickerPlacement);
	if (!fast) return null;
	const on = value ?? fast.defaultOn;
	const label = fast.label || t("fastMode.label");
	if (placement === "sheet")
		return (
			<div className="flex min-h-12 items-center gap-3 px-3 text-sm">
				<Zap
					className={cn(
						"size-4 shrink-0",
						on ? "text-warning fill-current" : "text-muted-foreground",
					)}
					aria-hidden
				/>
				<span className="min-w-0 flex-1">
					<span className="block">{label}</span>
					{fast.description ? (
						<span className="text-muted-foreground block text-xs">{fast.description}</span>
					) : null}
				</span>
				<Switch label={label} checked={on} disabled={disabled} onCheckedChange={onChange} />
			</div>
		);
	return (
		<button
			type="button"
			aria-pressed={on}
			aria-label={label}
			title={`${on ? t("fastMode.on", { label }) : t("fastMode.off", { label })}${fast.description ? ` · ${fast.description}` : ""}`}
			disabled={disabled}
			onClick={() => onChange(!on)}
			className={cn(
				"hover:bg-accent focus-visible:ring-ring grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2 disabled:opacity-50",
				on ? "text-warning" : "text-muted-foreground",
			)}
		>
			<Zap className={cn("size-3.5", on && "fill-current")} />
		</button>
	);
}
