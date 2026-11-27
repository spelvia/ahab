import { formatNumber, t } from "@/shared/i18n";

/** A number with `digits` decimals, as the language writes it: "12.3". */
export function decimal(value: number, digits: number): string {
	return formatNumber(value, { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** Bytes as people read them: "512 MB", "14.2 GB", "1.8 TB". */
export function bytes(value: number): string {
	const units = ["B", "KB", "MB", "GB", "TB"];
	let size = value;
	let unit = 0;
	while (size >= 1024 && unit < units.length - 1) {
		size /= 1024;
		unit += 1;
	}
	return `${size >= 100 || unit === 0 ? Math.round(size) : decimal(size, 1)} ${units[unit]}`;
}

/** A share of a whole, 0 to 100; 0 when the whole is unknown. */
export function share(part: number, whole: number): number {
	return whole > 0 ? Math.min(100, (part / whole) * 100) : 0;
}

/** How long, from seconds: "3 d 4 h", "5 h 12 min", "8 min". */
export function duration(seconds: number): string {
	const minutes = Math.floor(seconds / 60);
	const hours = Math.floor(minutes / 60);
	const days = Math.floor(hours / 24);
	if (days > 0) return t("units.duration.days", { ns: "machines", days, hours: hours % 24 });
	if (hours > 0) return t("units.duration.hours", { ns: "machines", hours, minutes: minutes % 60 });
	return t("units.duration.minutes", { ns: "machines", minutes });
}

/** How pressing a share is: amber past three quarters, red past nine tenths. */
export function toneOf(percent: number): "ok" | "warning" | "danger" {
	if (percent >= 90) return "danger";
	if (percent >= 75) return "warning";
	return "ok";
}

/** CPU time as htop shows it: "12.3s", "4:05" (minutes), "3h 12m". */
export function cpuTime(milliseconds: number): string {
	const seconds = milliseconds / 1000;
	if (seconds < 60)
		return t("units.cpuTime.seconds", {
			ns: "machines",
			seconds: decimal(seconds, 1),
		});
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
	return t("units.cpuTime.hours", {
		ns: "machines",
		hours: Math.floor(minutes / 60),
		minutes: minutes % 60,
	});
}
