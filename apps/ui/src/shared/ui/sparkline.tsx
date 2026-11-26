import { cn } from "cn";

/**
 * A small line of recent values, oldest left, filled below, scaled to
 * `max` (or the largest value). Decorative: the number beside it says it.
 */
export function Sparkline({
	values,
	max,
	className,
}: {
	values: readonly number[];
	/** The top of the scale; the largest value when absent. */
	max?: number;
	className?: string;
}) {
	if (values.length < 2) return <span aria-hidden className={cn("block h-6", className)} />;
	const top = Math.max(max ?? 0, ...values, 1e-9);
	const width = 100;
	const height = 24;
	const step = width / (values.length - 1);
	const points = values.map(
		(value, index) =>
			`${(index * step).toFixed(2)},${(height - (Math.max(0, value) / top) * height).toFixed(2)}`,
	);
	return (
		<svg
			aria-hidden
			viewBox={`0 0 ${width} ${height}`}
			preserveAspectRatio="none"
			className={cn("block h-6 w-full overflow-visible", className)}
		>
			<polygon
				points={`0,${height} ${points.join(" ")} ${width},${height}`}
				className="fill-current opacity-15"
			/>
			<polyline
				points={points.join(" ")}
				fill="none"
				vectorEffect="non-scaling-stroke"
				strokeWidth={1.5}
				strokeLinejoin="round"
				className="stroke-current"
			/>
		</svg>
	);
}
