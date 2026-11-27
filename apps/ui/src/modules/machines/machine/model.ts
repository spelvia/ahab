import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatDate, t } from "@/shared/i18n";

/** What a person sees first about a machine. */
export type MachineStatus = "online" | "offline" | "pending";

export function statusOf(machine: Machine): MachineStatus {
	if (machine.state === "pending") return "pending";
	return machine.online ? "online" : "offline";
}

export function onlineCount(machines: readonly Machine[]) {
	return machines.filter((machine) => machine.online).length;
}

/** The machines that wait for approval, oldest request first. */
export function waiting(machines: readonly Machine[]) {
	return machines
		.filter((machine) => machine.state === "pending")
		.toSorted((a, b) => a.createdAt.localeCompare(b.createdAt));
}

/** The approved machines, as the server lists them (its own first). */
export function approved(machines: readonly Machine[]) {
	return machines.filter((machine) => machine.state !== "pending");
}

/** "Connected" while its daemon is, else when it was last seen, as a label. */
export function presence(machine: Machine, now: Date = new Date()) {
	if (machine.online) return t("presence.connected", { ns: "machines" });
	const seen = lastSeen(machine, now);
	return seen.charAt(0).toUpperCase() + seen.slice(1);
}

/** A time as a person reads it in full, for titles and details. */
export function fullTime(value: string | null | undefined) {
	if (!value) return "–";
	return formatDate(value, {
		dateStyle: "medium",
		timeStyle: "short",
	});
}

/** When the machine was last seen, in words: "just now", "5 min ago", a date. */
export function lastSeen(machine: Machine, now: Date = new Date()) {
	return machine.lastSeenAt
		? agoInWords(machine.lastSeenAt, now)
		: t("presence.never", { ns: "machines" });
}

/** Why `name` cannot be a machine's name, or `undefined` when it can. The server checks too. */
export function nameProblem(name: string) {
	if (name.length === 0) return t("nameProblem.empty", { ns: "machines" });
	if (name.length > 63) return t("nameProblem.tooLong", { ns: "machines" });
	if (!/^[a-z0-9-]+$/.test(name)) return t("nameProblem.characters", { ns: "machines" });
	if (name.startsWith("-") || name.endsWith("-"))
		return t("nameProblem.hyphenAtEnds", { ns: "machines" });
	return undefined;
}

/** "linux · x86_64", or an empty string before the daemon reported it. */
export function platformOf(machine: Machine) {
	return [machine.os, machine.arch].filter(Boolean).join(" · ");
}
