import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { AgentSummary } from "@/modules/machines/agents";
import { MachineMenu } from "@/modules/machines/machine/components/machine-menu";
import { RemoveConfirmation } from "@/modules/machines/machine/components/remove-confirmation";
import { RenameForm } from "@/modules/machines/machine/components/rename-form";
import { StatusDot } from "@/modules/machines/machine/components/status-dot";
import { fullTime, platformOf, presence, statusOf } from "@/modules/machines/machine/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";

/**
 * The approved machines as one calm, dense table: status, platform, the
 * agents each can run, version and last seen, each with its ⋯ menu. A
 * machine's name opens its page.
 */
export function MachineTable({ machines }: { machines: readonly Machine[] }) {
	const { t } = useTranslation("machines");
	return (
		<div>
			<table className="w-full border-collapse text-sm">
				<thead>
					<tr className="border-border text-muted-foreground border-b text-left text-xs font-medium">
						<th scope="col" className="py-2 pr-4 pl-3 font-medium">
							{t("table.machine")}
						</th>
						<th scope="col" className="hidden py-2 pr-4 font-medium md:table-cell">
							{t("table.platform")}
						</th>
						<th scope="col" className="py-2 pr-4 font-medium">
							{t("table.agents")}
						</th>
						<th scope="col" className="hidden py-2 pr-4 font-medium lg:table-cell">
							{t("table.version")}
						</th>
						<th scope="col" className="hidden py-2 pr-4 font-medium sm:table-cell">
							{t("table.lastSeen")}
						</th>
						<th scope="col" className="w-10 py-2 pr-2">
							<span className="sr-only">{t("table.actions")}</span>
						</th>
					</tr>
				</thead>
				<tbody>
					{machines.map((machine) => (
						<MachineRow key={machine.id} machine={machine} />
					))}
				</tbody>
			</table>
		</div>
	);
}

function MachineRow({ machine }: { machine: Machine }) {
	const { t } = useTranslation("machines");
	const [mode, setMode] = useState<"view" | "rename" | "remove">("view");
	const status = statusOf(machine);
	// Back from renaming or removing, focus returns to the machine's name.
	const nameLink = useRef<HTMLAnchorElement>(null);
	const wasBusy = useRef(false);
	useEffect(() => {
		if (mode === "view" && wasBusy.current) nameLink.current?.focus();
		wasBusy.current = mode !== "view";
	}, [mode]);
	const platform = platformOf(machine);

	if (mode === "remove") {
		return (
			<tr className="border-border bg-destructive/6 border-b">
				<td colSpan={6} className="px-3 py-3">
					<RemoveConfirmation machine={machine} onCancel={() => setMode("view")} />
				</td>
			</tr>
		);
	}

	return (
		<tr className="border-border hover:bg-accent/40 group border-b align-middle transition-colors duration-150 last:border-b-0">
			<td className="py-3 pr-4 pl-3">
				{mode === "rename" ? (
					<RenameForm machine={machine} onDone={() => setMode("view")} />
				) : (
					<div className="flex items-center gap-3">
						<StatusDot status={status} />
						<div className="min-w-0">
							<div className="flex flex-wrap items-center gap-2">
								<Link
									ref={nameLink}
									to="/settings/machines/$id"
									params={{ id: machine.id }}
									className="text-heading font-semibold underline-offset-4 hover:underline"
								>
									{machine.name}
								</Link>
								{machine.isLocal && (
									<span className="pill pill-muted">{t("table.thisServer")}</span>
								)}
							</div>
							{platform && (
								<p className="text-muted-foreground mt-0.5 text-xs md:hidden">{platform}</p>
							)}
						</div>
					</div>
				)}
			</td>
			<td className="text-muted-foreground hidden py-3 pr-4 md:table-cell">{platform || "–"}</td>
			<td className="py-3 pr-4">
				<AgentSummary machineId={machine.id} />
			</td>
			<td className="text-muted-foreground hidden py-3 pr-4 lg:table-cell">
				{machine.version ? <span className="font-mono text-xs">{machine.version}</span> : "–"}
			</td>
			<td
				className={
					machine.online
						? "text-foreground hidden py-3 pr-4 sm:table-cell"
						: "text-muted-foreground hidden py-3 pr-4 sm:table-cell"
				}
				title={fullTime(machine.lastSeenAt)}
			>
				{presence(machine)}
			</td>
			<td className="py-3 pr-2 text-right">
				<MachineMenu
					machine={machine}
					onRename={() => setMode("rename")}
					onRemove={() => setMode("remove")}
				/>
			</td>
		</tr>
	);
}
