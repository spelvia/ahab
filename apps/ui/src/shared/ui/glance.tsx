import type { ReactNode } from "react";

/** One count in the glance row, opening the tab it sums up. */
export function GlanceItem({
	label,
	value,
	detail,
	link,
}: {
	label: string;
	value: ReactNode;
	detail?: ReactNode;
	link?: ReactNode;
}) {
	return (
		<li className="border-border space-y-0.5 rounded-lg border px-3 py-2.5">
			<p className="text-muted-foreground text-xs">{label}</p>
			<p className="text-heading text-lg font-semibold tabular-nums">{value}</p>
			{detail && <p className="text-muted-foreground truncate text-xs">{detail}</p>}
			{link && <p className="text-xs">{link}</p>}
		</li>
	);
}
