import { cn } from "cn";
import { Gauge } from "lucide-react";

import { UsagePanel, usePlanUsage } from "@/modules/machines";
import { type ContextUse, tokens } from "@/modules/sessions/chat/model";
import { useAppearance } from "@/shared/appearance";
import { formatNumber, useTranslation } from "@/shared/i18n";
import { PopoverSheet } from "@/shared/ui/popover-sheet";

type Tone = "ok" | "warning" | "danger";
const stroke: Record<Tone, string> = {
	ok: "stroke-primary",
	warning: "stroke-warning",
	danger: "stroke-destructive",
};
const text: Record<Tone, string> = {
	ok: "text-muted-foreground",
	warning: "text-warning",
	danger: "text-destructive",
};

function toneOf(share: number): Tone {
	if (share > 0.9) return "danger";
	if (share > 0.75) return "warning";
	return "ok";
}

/** A meter's trigger: 44px on phones, compact in the toolbar elsewhere. */
const meterTrigger =
	"hover:bg-accent focus-visible:ring-ring text-muted-foreground flex h-11 min-w-11 items-center justify-center gap-1.5 rounded-md px-1.5 text-xs tabular-nums outline-none focus-visible:ring-2 sm:h-7 sm:min-w-7";

/** What a session has spent, as its agent reported it. */
export interface SessionSpend {
	tokens?: number | null;
	costUsd?: number | null;
}

/** US dollars, to the cent above a dollar and finer below. */
function dollars(cost: number): string {
	return formatNumber(cost, {
		style: "currency",
		currency: "USD",
		maximumFractionDigits: cost >= 1 ? 2 : 4,
	});
}

/**
 * The session's usage beside the composer: how full the agent's context is
 * (a ring, amber then red as it fills; it spins while the agent compacts),
 * shown as a ring alone. The details are a tap away, in a popover or a sheet on
 * phones: the context and, as Settings → Agent editor says, the tokens the
 * session spent and its cost.
 */
export function SessionUsageMeter({
	context,
	compacting = false,
	spend,
}: {
	context: ContextUse | undefined;
	compacting?: boolean;
	spend?: SessionSpend;
}) {
	const { t } = useTranslation("sessions");
	const { showCost, showTokens } = useAppearance();
	const spent = showTokens ? (spend?.tokens ?? undefined) : undefined;
	const cost = showCost ? (spend?.costUsd ?? undefined) : undefined;
	if (!context && !compacting && spent === undefined && cost === undefined) return null;
	const used = context?.used ?? 0;
	const window = context?.window ?? null;
	// More than the window is a miscount, not a full context.
	const known = Boolean(context && !context.stale && !compacting && (!window || used <= window));
	const share = known && window ? used / window : undefined;
	const tone = share === undefined ? "ok" : toneOf(share);
	const circumference = 2 * Math.PI * 6;
	const contextSummary = compacting
		? t("chatMeters.compacting")
		: context?.stale
			? t("chatMeters.compacted")
			: share !== undefined
				? t("chatMeters.contextOf", { percent: Math.round(share * 100) })
				: context
					? t("chatMeters.contextUsed", { used: tokens(used) })
					: undefined;
	const label = t("chatMeters.usageLabel", {
		summary: [
			contextSummary,
			spent !== undefined ? t("chatMeters.spentShort", { tokens: tokens(spent) }) : undefined,
			cost !== undefined ? dollars(cost) : undefined,
		]
			.filter(Boolean)
			.join(", "),
	});
	return (
		<PopoverSheet
			title={t("chatMeters.usage")}
			label={label}
			triggerClassName={meterTrigger}
			contentClassName="w-72"
			trigger={
				<>
					<svg
						viewBox="0 0 16 16"
						className={cn("size-4 shrink-0 -rotate-90", compacting && "motion-safe:animate-spin")}
						aria-hidden
					>
						<circle cx="8" cy="8" r="6" fill="none" strokeWidth="2.5" className="stroke-border" />
						{compacting ? (
							<circle
								cx="8"
								cy="8"
								r="6"
								fill="none"
								strokeWidth="2.5"
								strokeLinecap="round"
								strokeDasharray={`${0.3 * circumference} ${circumference}`}
								className="stroke-primary"
							/>
						) : share !== undefined ? (
							<circle
								cx="8"
								cy="8"
								r="6"
								fill="none"
								strokeWidth="2.5"
								strokeLinecap="round"
								strokeDasharray={`${share * circumference} ${circumference}`}
								className={cn("transition-[stroke-dasharray] duration-300", stroke[tone])}
							/>
						) : null}
					</svg>
				</>
			}
		>
			<div className="space-y-3">
				<p className="text-heading hidden text-sm font-semibold sm:block">
					{t("chatMeters.usage")}
				</p>
				{context || compacting ? (
					<section className="space-y-1.5">
						<div className="flex items-baseline justify-between gap-3">
							<h3 className="text-sm">{t("chatMeters.context")}</h3>
							{share !== undefined ? (
								<span className={cn("text-xs tabular-nums", text[tone])}>
									{Math.round(share * 100)}%
								</span>
							) : null}
						</div>
						{share !== undefined ? (
							<div
								role="meter"
								aria-valuemin={0}
								aria-valuemax={100}
								aria-valuenow={Math.round(share * 100)}
								aria-label={t("chatMeters.context")}
								className="bg-muted h-1.5 overflow-hidden rounded-full"
							>
								<div
									className={cn("h-full rounded-full", stroke[tone].replace("stroke-", "bg-"))}
									style={{ width: `${share * 100}%` }}
								/>
							</div>
						) : null}
						{compacting || context?.stale ? (
							<p className="text-muted-foreground text-xs">
								{compacting ? t("chatMeters.compactingDetail") : t("chatMeters.compactedDetail")}
							</p>
						) : (
							<>
								<p className="text-muted-foreground text-xs tabular-nums">
									{window
										? t("chatMeters.tokensOf", {
												used: formatNumber(used),
												window: formatNumber(window),
											})
										: t("chatMeters.tokens", { used: formatNumber(used) })}
								</p>
								<p className="text-muted-foreground text-xs">{t("chatMeters.asOfCall")}</p>
							</>
						)}
					</section>
				) : null}
				{spent !== undefined || cost !== undefined ? (
					<dl className="border-border divide-border divide-y border-t text-sm">
						{spent !== undefined ? (
							<div className="flex items-baseline justify-between gap-3 py-2">
								<dt>{t("chatMeters.spent")}</dt>
								<dd className="text-heading tabular-nums">{formatNumber(spent)}</dd>
							</div>
						) : null}
						{cost !== undefined ? (
							<div className="flex items-baseline justify-between gap-3 py-2">
								<dt>{t("chatMeters.cost")}</dt>
								<dd className="text-heading tabular-nums">{dollars(cost)}</dd>
							</div>
						) : null}
					</dl>
				) : null}
			</div>
		</PopoverSheet>
	);
}

/**
 * How much of its plan the agent has used, for agents with plan limits: a
 * gauge, amber then red as a limit nears; every limit a tap away.
 */
export function PlanMeter({ machineId, agent }: { machineId: string; agent: string }) {
	const { t } = useTranslation("sessions");
	const plan = usePlanUsage(machineId, agent);
	if (!plan) return null;
	const summary = t("chatMeters.planTone", { context: plan.tone });
	return (
		<PopoverSheet
			title={t("chatMeters.planTitle")}
			label={t("chatMeters.plan", { summary })}
			triggerClassName={meterTrigger}
			contentClassName="w-80"
			trigger={<Gauge className={cn("size-4", text[plan.tone])} aria-hidden />}
		>
			<UsagePanel machineId={machineId} usage={plan.usage} />
		</PopoverSheet>
	);
}
