/** Times and sizes in words, the same everywhere. */

import { formatDate, t } from "@/shared/i18n";

function millis(when: Date | string | number): number {
	return when instanceof Date ? when.getTime() : typeof when === "string" ? Date.parse(when) : when;
}

/** How long ago, in a few characters: "now", "4m", "3h", "2d", "1y". `when` in milliseconds when a number. */
export function ago(when: Date | string | number, now: Date | number = Date.now()): string {
	const minutes = Math.floor((millis(now) - millis(when)) / 60_000);
	if (minutes < 1) return t("time.short.now");
	if (minutes < 60) return t("time.short.minutes", { value: minutes });
	if (minutes < 60 * 24) return t("time.short.hours", { value: Math.floor(minutes / 60) });
	if (minutes < 60 * 24 * 365)
		return t("time.short.days", { value: Math.floor(minutes / 60 / 24) });
	return t("time.short.years", { value: Math.floor(minutes / 60 / 24 / 365) });
}

/** How long ago, in words: "just now", "5 min ago", "3 h ago", then the date. */
export function agoInWords(when: Date | string | number, now: Date | number = Date.now()): string {
	const minutes = Math.floor((millis(now) - millis(when)) / 60_000);
	if (minutes < 1) return t("time.justNow");
	if (minutes < 60) return t("time.minutesAgo", { count: minutes });
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return t("time.hoursAgo", { count: hours });
	return formatDate(millis(when), {
		day: "numeric",
		month: "short",
		year: "numeric",
	});
}

/** A size in bytes, as "512 B", "14 KB", "2.5 MB". */
export function byteSize(bytes: number): string {
	if (bytes < 1024) return t("size.bytes", { value: bytes });
	if (bytes < 1024 * 1024) return t("size.kilobytes", { value: Math.round(bytes / 1024) });
	return t("size.megabytes", { value: (bytes / 1024 / 1024).toFixed(1) });
}
