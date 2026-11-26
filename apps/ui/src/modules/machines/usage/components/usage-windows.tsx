import { cn } from "cn";

import { percent, resetsIn, toneBar, toneOf, toneText } from "@/modules/machines/usage/model";
import type { AgentUsage } from "@/shared/api/generated/ahab.schemas";
import { formatDate, useTranslation } from "@/shared/i18n";

/** A time in full, as `toLocaleString` writes it. */
const FULL: Intl.DateTimeFormatOptions = {
	year: "numeric",
	month: "numeric",
	day: "numeric",
	hour: "numeric",
	minute: "numeric",
	second: "numeric",
};

/**
 * An agent's plan limits as bars: how much of each is used and when it
 * starts over; or, when there is nothing to show, why.
 */
export function UsageWindows({ usage, className }: { usage: AgentUsage; className?: string }) {
	const { t } = useTranslation("machines");
	if (usage.status !== "available" || usage.windows.length === 0)
		return usage.problem ? (
			<p className={cn("text-muted-foreground text-xs", className)}>{usage.problem}</p>
		) : null;
	return (
		<div className={cn("space-y-1", className)}>
			<ul className="space-y-1" aria-label={t("usage.limitsLabel", { agent: usage.label })}>
				{usage.windows.map((window) => {
					const tone = toneOf(window.usedPercent);
					return (
						<li
							key={window.id}
							className="grid grid-cols-[minmax(0,8rem)_minmax(3rem,1fr)_2.75rem] items-center gap-x-3 text-xs sm:grid-cols-[minmax(0,10rem)_minmax(3rem,12rem)_2.75rem_auto]"
						>
							<span className="text-muted-foreground truncate" title={window.label}>
								{window.label}
							</span>
							<span
								className="bg-muted h-1.5 overflow-hidden rounded-full"
								role="meter"
								aria-label={window.label}
								aria-valuemin={0}
								aria-valuemax={100}
								aria-valuenow={Math.round(window.usedPercent)}
							>
								<span
									className={cn("block h-full rounded-full", toneBar[tone])}
									style={{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }}
								/>
							</span>
							<span
								className={cn(
									"text-right tabular-nums",
									tone === "ok" ? "text-foreground" : toneText[tone],
								)}
							>
								{percent(window)}
							</span>
							<span
								className="text-muted-foreground col-span-3 whitespace-nowrap sm:col-span-1"
								title={window.resetsAt ? formatDate(window.resetsAt, FULL) : undefined}
							>
								{resetsIn(window.resetsAt)}
							</span>
						</li>
					);
				})}
			</ul>
			{usage.problem && <p className="text-muted-foreground text-xs">{usage.problem}</p>}
		</div>
	);
}
