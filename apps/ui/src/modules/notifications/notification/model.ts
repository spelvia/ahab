import type { Notification as AhabNotification } from "@/shared/api/generated/ahab.schemas";
import { formatDate, t as translate } from "@/shared/i18n";

/** What opening a notification shows, as an address of this app; none for unknown targets. */
export function targetPath(target: unknown): string | undefined {
	if (!target || typeof target !== "object") return undefined;
	const t = target as Record<string, unknown>;
	if (t.type === "machine") {
		return typeof t.machineId === "string" ? `/settings/machines/${t.machineId}` : undefined;
	}
	// A page of Ahab a plugin names; never another site (`//host`).
	if (t.type === "route") {
		return typeof t.path === "string" && t.path.startsWith("/") && !t.path.startsWith("//")
			? t.path
			: undefined;
	}
	if (t.type !== "session") return undefined;
	const { projectId, sessionId, chatId } = t;
	if (typeof projectId !== "string" || typeof sessionId !== "string") return undefined;
	// The session opens at the chat it is about.
	const chat = typeof chatId === "string" ? `?chat=${encodeURIComponent(chatId)}` : "";
	return `/p/${projectId}/s/${sessionId}${chat}`;
}

/** A page outside Ahab the notification opens (a release, say); only http(s). */
export function targetUrl(target: unknown): string | undefined {
	if (!target || typeof target !== "object") return undefined;
	const t = target as Record<string, unknown>;
	if (t.type !== "url" || typeof t.url !== "string") return undefined;
	try {
		const url = new URL(t.url);
		return url.protocol === "https:" || url.protocol === "http:" ? url.href : undefined;
	} catch {
		return undefined;
	}
}

/** Whether `path` already shows what the notification points at. */
export function showing(path: string, target: unknown): boolean {
	const wanted = targetPath(target);
	if (!wanted) return false;
	const session = wanted.split("?")[0]?.split("/s/")[1];
	return Boolean(session) && path.endsWith(`/s/${session}`);
}

function dayStart(date: Date) {
	return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/** The inbox in days, newest first: Today, Yesterday, then dates. */
export function byDay(items: AhabNotification[], now = new Date()) {
	const today = dayStart(now);
	const days = new Map<string, AhabNotification[]>();
	for (const item of items) {
		const created = new Date(item.createdAt);
		const start = dayStart(created);
		const label =
			start === today
				? translate("inbox.today", { ns: "notifications" })
				: start === today - 86_400_000
					? translate("inbox.yesterday", { ns: "notifications" })
					: formatDate(created, {
							day: "numeric",
							month: "short",
							year: "numeric",
						});
		days.set(label, [...(days.get(label) ?? []), item]);
	}
	return [...days.entries()].map(([label, list]) => ({ label, items: list }));
}

/** A short time of day, as the inbox shows it. */
export function timeOf(createdAt: string) {
	return formatDate(createdAt, { hour: "2-digit", minute: "2-digit" });
}

/** Settings' kinds in groups: Ahab's first, then each plugin's, by name. */
export function bySource<Kind extends { source: string; sourceName: string }>(kinds: Kind[]) {
	const groups = new Map<string, { name: string; kinds: Kind[] }>();
	for (const kind of kinds) {
		const group = groups.get(kind.source) ?? { name: kind.sourceName, kinds: [] };
		group.kinds.push(kind);
		groups.set(kind.source, group);
	}
	return [...groups.entries()]
		.toSorted(([a, left], [b, right]) =>
			a === "ahab" ? -1 : b === "ahab" ? 1 : left.name.localeCompare(right.name),
		)
		.map(([source, group]) => ({ source, name: group.name, kinds: group.kinds }));
}

/**
 * Where a chat's notification happened, as one line: `ahab · Fix the test`
 * (its project and session, as named when it was sent).
 * The session's title is left out when it is the body already.
 */
export function placeOf(
	notification: Pick<AhabNotification, "target" | "body">,
): string | undefined {
	const target = notification.target as Record<string, unknown> | null | undefined;
	const name = (field: string) =>
		typeof target?.[field] === "string" && target[field] ? (target[field] as string) : undefined;
	const session = name("sessionTitle");
	const parts = [name("projectName"), session === notification.body ? undefined : session].filter(
		(part): part is string => Boolean(part),
	);
	return parts.length ? parts.join(" · ") : undefined;
}
