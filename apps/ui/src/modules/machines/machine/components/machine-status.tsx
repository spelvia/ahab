import { Link } from "@tanstack/react-router";
import { cn } from "cn";

import { useMachines } from "@/modules/machines/machine/api";
import { StatusDot } from "@/modules/machines/machine/components/status-dot";
import { presence, statusOf } from "@/modules/machines/machine/model";
import { useTranslation } from "@/shared/i18n";
import { statusItemClass } from "@/shared/ui/status-bar";

/**
 * The status bar's machine: `● atlas`, its connection at a glance (offline
 * and outdated said in words); a click opens its page in Settings.
 */
export function MachineStatusItem({
	machineId,
	name,
	wide = false,
	className,
}: {
	machineId: string;
	/** Its name as the project knows it, until the list loads. */
	name: string;
	/** Room to say its connection in words, online too (a phone's sheet). */
	wide?: boolean;
	className?: string;
}) {
	const { t } = useTranslation("machines");
	const machines = useMachines();
	const machine = machines.data?.items.find((item) => item.id === machineId);
	const status = machine ? statusOf(machine) : "offline";
	const outdated = Boolean(machine?.outdated);
	const shownName = machine?.name ?? name;
	const state = !machine
		? ""
		: outdated
			? t("statusItem.outdatedState")
			: presence(machine).toLowerCase();
	const label = state
		? t("statusItem.label", { name: shownName, state })
		: t("statusItem.labelWithoutState", { name: shownName });
	return (
		<Link
			to="/settings/machines/$id"
			params={{ id: machineId }}
			aria-label={label}
			title={label}
			className={cn(statusItemClass, "gap-1.5", outdated && "text-warning", className)}
		>
			<StatusDot status={status} className="size-2" />
			<span className={cn("truncate", wide ? "min-w-0 text-foreground" : "max-w-40")}>
				{shownName}
			</span>
			{outdated ? (
				<span className="shrink-0">{t("statusItem.outdated")}</span>
			) : wide && state ? (
				<span className="shrink-0">{state}</span>
			) : status === "offline" ? (
				<span className="shrink-0">{t("statusItem.offline")}</span>
			) : null}
		</Link>
	);
}

/** The machine's connection as a dot alone, for a phone's top bar. */
export function MachineStatusDot({
	machineId,
	className,
}: {
	machineId: string;
	className?: string;
}) {
	const machines = useMachines();
	const machine = machines.data?.items.find((item) => item.id === machineId);
	return <StatusDot status={machine ? statusOf(machine) : "offline"} className={className} />;
}
