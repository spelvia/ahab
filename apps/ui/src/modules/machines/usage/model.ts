import type { AgentUsage, UsageWindow } from "@/shared/api/generated/ahab.schemas";
import { formatDate, t } from "@/shared/i18n";

/** The agents whose plan limits Ahab can read. */
export const METERED = new Set(["claude-code", "codex"]);

/** How pressing a limit is: amber past three quarters, red past nine tenths (as the context meter). */
export type Tone = "ok" | "warning" | "danger";

export function toneOf(usedPercent: number): Tone {
	if (usedPercent >= 90) return "danger";
	if (usedPercent >= 75) return "warning";
	return "ok";
}

export const toneText: Record<Tone, string> = {
	ok: "text-muted-foreground",
	warning: "text-warning",
	danger: "text-destructive",
};

export const toneBar: Record<Tone, string> = {
	ok: "bg-primary",
	warning: "bg-warning",
	danger: "bg-destructive",
};

/** The limits worth a glance: those with a short name (the session, the week). */
export function summary(usage: AgentUsage): UsageWindow[] {
	return usage.windows.filter((window) => window.shortLabel);
}

/** The most pressing tone among `windows`. */
export function worst(windows: readonly UsageWindow[]): Tone {
	const most = Math.max(0, ...windows.map((window) => window.usedPercent));
	return toneOf(most);
}

export function percent(window: UsageWindow): string {
	return `${Math.round(window.usedPercent)}%`;
}

/** When a limit starts over, in words: "resets in 53 min", "in 4 h", "on Wed 18 Nov". */
export function resetsIn(resetsAt: string | null | undefined, now: number = Date.now()): string {
	if (!resetsAt) return "";
	const minutes = Math.ceil((Date.parse(resetsAt) - now) / 60_000);
	if (minutes <= 0) return t("usage.resetsNow", { ns: "machines" });
	if (minutes < 60) return t("usage.resetsInMinutes", { ns: "machines", minutes });
	const hours = Math.round(minutes / 60);
	if (hours < 24) return t("usage.resetsInHours", { ns: "machines", hours });
	return t("usage.resetsOn", {
		ns: "machines",
		date: formatDate(resetsAt, { weekday: "short", day: "numeric", month: "short" }),
	});
}
