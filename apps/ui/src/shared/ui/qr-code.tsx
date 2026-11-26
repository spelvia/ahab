import { encodeQR } from "qr";
import { useMemo } from "react";

/** `text` as a QR code, dark on white whatever the theme, for a phone to scan. */
export function QrCode({
	text,
	label,
	className = "size-48",
}: {
	text: string;
	label: string;
	className?: string;
}) {
	const { size, path } = useMemo(() => {
		const cells = encodeQR(text, "raw", { ecc: "medium", border: 2 });
		let d = "";
		cells.forEach((row, y) =>
			row.forEach((dark, x) => {
				if (dark) d += `M${x} ${y}h1v1h-1z`;
			}),
		);
		return { size: cells.length, path: d };
	}, [text]);
	return (
		<svg
			role="img"
			aria-label={label}
			viewBox={`0 0 ${size} ${size}`}
			shapeRendering="crispEdges"
			className={`rounded-md bg-white ${className}`}
		>
			<path d={path} fill="#000" />
		</svg>
	);
}
