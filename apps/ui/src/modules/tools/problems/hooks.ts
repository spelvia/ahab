import { useSyncExternalStore } from "react";

import { countText } from "@/modules/tools/problems/model";
import type { CheckRunDto } from "@/shared/api/generated/ahab.schemas";
import { formatDate, t } from "@/shared/i18n";

// Check runs asked for in this browser (Check project), as lines for the
// Output tab, per project. The daemon keeps no log of them: what a run
// says is its answer.

/** Lines kept per project. */
const KEPT = 1_000;
const runs = new Map<string, string[]>();
const listeners = new Set<() => void>();
const NONE: string[] = [];

function time(at: Date): string {
	// A time as `toLocaleTimeString` writes it, in 24 hours.
	return formatDate(at, { hour: "numeric", minute: "2-digit", second: "2-digit", hour12: false });
}

/** A check run's answer, as lines. */
export function checkRunLines(
	checks: readonly CheckRunDto[] | undefined,
	error: string | undefined,
	at: Date = new Date(),
): string[] {
	const stamp = `[${time(at)}]`;
	if (error) return [`${stamp} ${t("tools:checkLog.failed", { error })}`];
	if (!checks?.length) return [`${stamp} ${t("tools:checkLog.nothing")}`];
	return [
		`${stamp} ${t("tools:checkLog.summary", { count: checks.length })}`,
		...checks.map((check) => {
			const outcome = check.ok
				? countText(check.counts)
				: t("tools:checkLog.checkFailed", {
						reason: check.message ?? t("tools:checkLog.noReason"),
					});
			const line =
				check.root && check.root !== "."
					? t("tools:checkLog.checkIn", { name: check.name, root: check.root, outcome })
					: t("tools:checkLog.check", { name: check.name, outcome });
			return `${stamp}   ${check.ok ? "✓" : "✗"} ${line}`;
		}),
	];
}

export const checkLog = {
	add(projectId: string, lines: string[]) {
		runs.set(projectId, [...(runs.get(projectId) ?? []), ...lines].slice(-KEPT));
		for (const listener of listeners) listener();
	},
};

/** A project's check runs in this browser, as lines, kept up to date. */
export function useCheckLog(projectId: string | undefined): string[] {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		() => (projectId ? (runs.get(projectId) ?? NONE) : NONE),
		() => NONE,
	);
}
