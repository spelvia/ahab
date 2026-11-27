import { useEffect, useState } from "react";

import { toolTarget } from "@/modules/sessions/chat/components/tool-detail";
import { type Entry, workedFor } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";

/**
 * Activity at the foot of the running turn. Not a live region: the chat
 * announces a turn's start and end, and each tool call is not read out.
 */
export function WorkingLine({
	agent,
	since,
	entries,
	label,
}: {
	agent: string;
	since?: number;
	entries?: Entry[];
	/** What it waits for when no tool is running, such as a message being sent. */
	label?: string;
}) {
	const { t } = useTranslation("sessions");
	const latest = entries?.findLast(
		(entry) =>
			entry.kind === "item" &&
			(entry.item.type === "toolCall" || entry.item.type === "userMessage"),
	);
	const activity =
		latest?.kind === "item" &&
		latest.item.type === "toolCall" &&
		latest.item.status === "inProgress"
			? toolTarget(latest.item.detail)
			: label;
	const [mounted] = useState(() => Date.now());
	const [now, setNow] = useState(mounted);
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);
	return (
		<div className="text-muted-foreground flex items-center gap-2 py-1 text-xs tabular-nums">
			<span className="sr-only">{t("workingLine.working", { agent })}</span>
			<svg
				aria-hidden="true"
				viewBox="0 0 16 16"
				className="size-3.5 motion-safe:animate-spin"
				fill="currentColor"
			>
				{Array.from({ length: 8 }, (_, i) => (
					<circle
						key={i}
						cx="8"
						cy="2.5"
						r="1.2"
						transform={`rotate(${i * 45} 8 8)`}
						opacity={(i + 1) / 8}
					/>
				))}
			</svg>
			{activity && <span className="min-w-0 truncate">{activity}</span>}
			<span aria-hidden="true">{workedFor(Math.max(0, now - (since ?? mounted)))}</span>
		</div>
	);
}
