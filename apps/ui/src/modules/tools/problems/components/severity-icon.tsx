import { cn } from "cn";
import { CircleX, Info, Lightbulb, TriangleAlert } from "lucide-react";

import { rank } from "@/modules/tools/problems/model";
import { useTranslation } from "@/shared/i18n";

const SEVERITIES = ["error", "warning", "information", "hint"] as const;
const isSeverity = (value: string): value is (typeof SEVERITIES)[number] =>
	(SEVERITIES as readonly string[]).includes(value);

/** A problem's severity as an icon: error, warning, information, hint. */
export function SeverityIcon({ severity, className }: { severity: string; className?: string }) {
	const { t } = useTranslation("tools");
	const level = rank(severity);
	const Icon = [CircleX, TriangleAlert, Info, Lightbulb][level] ?? Lightbulb;
	return (
		<Icon
			aria-label={isSeverity(severity) ? t(`severity.${severity}`) : severity}
			className={cn(
				"size-3.5 shrink-0",
				level === 0 ? "text-destructive" : level === 1 ? "text-warning" : "text-muted-foreground",
				className,
			)}
		/>
	);
}
