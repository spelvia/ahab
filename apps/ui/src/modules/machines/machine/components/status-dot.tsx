import { cn } from "cn";

import type { MachineStatus } from "@/modules/machines/machine/model";
import { useTranslation } from "@/shared/i18n";

const tone: Record<MachineStatus, string> = {
	online: "bg-success shadow-[0_0_0_3px] shadow-success/20",
	offline: "border-muted-foreground/60 border bg-transparent",
	pending: "bg-warning shadow-[0_0_0_3px] shadow-warning/20",
};

/** A machine's status as a dot, with its word for screen readers. */
export function StatusDot({ status, className }: { status: MachineStatus; className?: string }) {
	const { t } = useTranslation("machines");
	const word = t(`status.${status}`);
	return (
		<span
			className={cn("inline-flex size-2.5 shrink-0 items-center justify-center", className)}
			title={word}
		>
			<span className={cn("size-2 rounded-full", tone[status])} />
			<span className="sr-only">{word}</span>
		</span>
	);
}
