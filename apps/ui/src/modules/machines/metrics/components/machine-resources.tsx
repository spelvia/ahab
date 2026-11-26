import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import { Puzzle, Server, SquareTerminal } from "lucide-react";
import { type ReactNode, useState } from "react";

import { AgentIcon, agentLabel } from "@/modules/machines/agents";
import { useMachineMetrics } from "@/modules/machines/metrics/api";
import { ProcessExplorer } from "@/modules/machines/metrics/components/process-explorer";
import { bytes, decimal, duration, share, toneOf } from "@/modules/machines/metrics/model";
import type { AhabUse, MachineMetrics } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatList, useTranslation } from "@/shared/i18n";
import { Sparkline } from "@/shared/ui/sparkline";

const bar = { ok: "bg-primary", warning: "bg-warning", danger: "bg-destructive" } as const;
const text = {
	ok: "text-foreground",
	warning: "text-warning",
	danger: "text-destructive",
} as const;
const line = { ok: "text-primary", warning: "text-warning", danger: "text-destructive" } as const;

/**
 * A machine's resources, live: four tiles answering "is it healthy?" (CPU,
 * memory, load, the fullest disk, each with its last ten minutes), then
 * what Ahab uses it for (agents, terminals, plugins, Ahab itself), then its
 * processes grouped by program; disks, cores and the system folded below.
 */
export function MachineResources({ machineId }: { machineId: string }) {
	const { t } = useTranslation("machines");
	const metrics = useMachineMetrics(machineId);

	if (metrics.isPending)
		return <p className="text-muted-foreground text-sm">{t("resources.reading")}</p>;
	if (metrics.isError)
		return (
			<p className="text-muted-foreground text-sm" role="status">
				{metrics.error.status === 503
					? t("resources.offline")
					: t("resources.readFailed", { message: metrics.error.message })}
			</p>
		);
	// Stale when the daemon's sampler fell behind, or asking again fails.
	const age = (metrics.dataUpdatedAt - Date.parse(metrics.data.sampledAt)) / 1000;
	return (
		<Loaded
			machineId={machineId}
			metrics={metrics.data}
			stale={metrics.isRefetchError || age > 15}
		/>
	);
}

function Loaded({
	machineId,
	metrics,
	stale,
}: {
	machineId: string;
	metrics: MachineMetrics;
	stale: boolean;
}) {
	const { t } = useTranslation("machines");
	const { cpu, memory, history } = metrics;
	const cores = Math.max(1, cpu.cores.length);
	const memoryShare = share(memory.used, memory.total);
	const load = cpu.load?.[0] ?? 0;
	const loadShare = share(load, cores);
	const disks = metrics.disks.map((disk) => ({
		...disk,
		used: share(disk.total - disk.available, disk.total),
	}));
	const fullest = disks.toSorted((a, b) => b.used - a.used)[0];
	const minutes = Math.round((history.length * 3) / 60);

	return (
		<div className="space-y-8">
			<p className="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
				<span
					className={cn("size-1.5 rounded-full", stale ? "bg-warning" : "bg-success")}
					aria-hidden
				/>
				{stale
					? t("resources.stale", { when: agoInWords(metrics.sampledAt) })
					: t("resources.live")}
				<span>
					· {[metrics.hostName, metrics.osVersion].filter(Boolean).join(" · ")} ·{" "}
					{t("resources.uptime", { duration: duration(metrics.uptimeSeconds) })}
					{minutes > 0 && ` · ${t("resources.chartSpan", { minutes })}`}
				</span>
			</p>

			<ul className="grid grid-cols-2 gap-3 lg:grid-cols-4" aria-label={t("resources.glance")}>
				<Tile
					label={t("resources.cpu")}
					value={`${Math.round(cpu.usagePercent)}%`}
					tone={toneOf(cpu.usagePercent)}
					values={history.map((sample) => sample.cpu)}
					max={100}
					detail={<CoreGrid cores={cpu.cores} />}
				/>
				<Tile
					label={t("resources.memory")}
					value={`${Math.round(memoryShare)}%`}
					tone={toneOf(memoryShare)}
					values={history.map((sample) => sample.memoryUsed)}
					max={memory.total}
					detail={[
						t("resources.memoryDetail", { used: bytes(memory.used), total: bytes(memory.total) }),
						memory.swapTotal > 0 &&
							t("resources.swap", {
								percent: Math.round(share(memory.swapUsed, memory.swapTotal)),
							}),
					]
						.filter(Boolean)
						.join(" · ")}
				/>
				<Tile
					label={t("resources.load")}
					value={decimal(load, 2)}
					tone={toneOf(loadShare)}
					values={history.map((sample) => sample.load)}
					max={cores}
					detail={
						cpu.load
							? t("resources.loadDetail", {
									cores,
									loads: formatList(
										cpu.load.slice(1).map((value) => decimal(value, 2)),
										"unit",
									),
								})
							: t("resources.loadUnknown")
					}
				/>
				<Tile
					label={fullest ? t("resources.diskAt", { mount: fullest.mount }) : t("resources.disk")}
					value={fullest ? `${Math.round(fullest.used)}%` : "–"}
					tone={fullest ? diskTone(fullest.used) : "ok"}
					detail={
						fullest
							? [
									t("resources.diskFree", { size: bytes(fullest.available) }),
									disks.length > 1 && t("resources.moreDisks", { more: disks.length - 1 }),
								]
									.filter(Boolean)
									.join(" · ")
							: t("resources.noDisks")
					}
				/>
			</ul>

			<AhabSection machineId={machineId} metrics={metrics} />

			<Panel
				initial={disks.some((disk) => disk.used >= 80) ? "disks" : "processes"}
				tabs={[
					{
						id: "processes",
						label: t("resources.tabs.processes", { total: metrics.processCount }),
						body: <ProcessExplorer metrics={metrics} />,
					},
					{
						id: "disks",
						label: t("resources.tabs.disks", { total: disks.length }),
						body: (
							<ul className="space-y-3">
								{disks.map((disk) => (
									<li key={disk.mount}>
										<Meter
											label={
												<>
													<span className="text-foreground font-mono">{disk.mount}</span>{" "}
													{t("resources.diskLine", {
														fileSystem: disk.fileSystem,
														free: bytes(disk.available),
														total: bytes(disk.total),
													})}
												</>
											}
											percent={disk.used}
											tone={diskTone(disk.used)}
										/>
									</li>
								))}
							</ul>
						),
					},
					{
						id: "cores",
						label: t("resources.tabs.cores", { total: cpu.cores.length }),
						body: (
							<ul className="grid grid-cols-2 gap-x-6 gap-y-1.5 sm:grid-cols-3 lg:grid-cols-4">
								{cpu.cores.map((core, index) => (
									// Cores keep their order.
									// oxlint-disable-next-line no-array-index-key
									<li key={index}>
										<Meter
											label={t("resources.core", { index })}
											percent={core}
											tone={toneOf(core)}
										/>
									</li>
								))}
							</ul>
						),
					},
					{
						id: "system",
						label: t("resources.tabs.system"),
						body: (
							<dl className="divide-border divide-y text-sm">
								<Pair label={t("resources.system.host")}>{metrics.hostName ?? "–"}</Pair>
								<Pair label={t("resources.system.system")}>{metrics.osVersion ?? "–"}</Pair>
								<Pair label={t("resources.system.kernel")}>{metrics.kernel ?? "–"}</Pair>
								<Pair label={t("resources.system.up")}>{duration(metrics.uptimeSeconds)}</Pair>
								<Pair label={t("resources.system.processes")}>{metrics.processCount}</Pair>
								<Pair label={t("resources.system.cores")}>
									{cpu.physicalCores
										? t("resources.system.logicalAndPhysical", {
												logical: cpu.cores.length,
												physical: cpu.physicalCores,
											})
										: t("resources.system.logical", { logical: cpu.cores.length })}
								</Pair>
							</dl>
						),
					},
				]}
			/>
		</div>
	);
}

function diskTone(used: number) {
	return used >= 95 ? "danger" : used >= 80 ? "warning" : "ok";
}

/** One number at a glance, with its last minutes and a line of detail. */
function Tile({
	label,
	value,
	tone,
	values,
	max,
	detail,
}: {
	label: string;
	value: string;
	tone: keyof typeof bar;
	values?: number[];
	max?: number;
	detail?: ReactNode;
}) {
	return (
		<li
			className={cn(
				"border-border min-w-0 space-y-1.5 rounded-lg border px-3 py-2.5",
				tone === "warning" && "border-warning/50",
				tone === "danger" && "border-destructive/50",
			)}
		>
			<div className="flex items-baseline justify-between gap-2">
				<span className="text-muted-foreground truncate text-xs">{label}</span>
				<span className={cn("text-lg font-semibold tabular-nums", text[tone])}>{value}</span>
			</div>
			{values && <Sparkline values={values} max={max} className={line[tone]} />}
			{detail && <div className="text-muted-foreground truncate text-xs">{detail}</div>}
		</li>
	);
}

/** Every core as a small square, darker as it gets busier. */
function CoreGrid({ cores }: { cores: number[] }) {
	const { t } = useTranslation("machines");
	return (
		<span
			className="flex flex-wrap gap-0.5"
			role="img"
			aria-label={t("resources.perCore", {
				percents: formatList(
					cores.map((core) => `${Math.round(core)}%`),
					"unit",
				),
			})}
		>
			{cores.map((core, index) => (
				<span
					// Cores keep their order.
					// oxlint-disable-next-line no-array-index-key
					key={index}
					title={t("resources.coreTitle", { index, percent: Math.round(core) })}
					className={cn("size-2 rounded-[2px]", bar[toneOf(core)])}
					style={{ opacity: 0.15 + Math.min(1, core / 100) * 0.85 }}
				/>
			))}
		</span>
	);
}

/** What Ahab uses the machine for: its agents, terminals, plugins and itself. */
function AhabSection({ machineId, metrics }: { machineId: string; metrics: MachineMetrics }) {
	const { t } = useTranslation("machines");
	const cpu = metrics.ahab.reduce((sum, group) => sum + group.cpuPercent, 0);
	const memory = metrics.ahab.reduce((sum, group) => sum + group.memoryBytes, 0);
	return (
		<section aria-labelledby="ahab-heading" className="space-y-2">
			<div className="flex flex-wrap items-end justify-between gap-3">
				<div>
					<h2 id="ahab-heading" className="text-base font-bold">
						{t("ahabUse.title")}
					</h2>
					<p className="text-muted-foreground text-sm">{t("ahabUse.description")}</p>
				</div>
				<div className="flex w-56 items-center gap-3 text-xs tabular-nums">
					<Sparkline
						values={metrics.history.map((sample) => sample.ahabCpu)}
						className="text-primary"
					/>
					<span className="text-foreground shrink-0">
						{decimal(cpu, 1)}% · {bytes(memory)}
					</span>
				</div>
			</div>
			{metrics.ahab.length === 0 ? (
				<p className="border-border text-muted-foreground border-y py-3 text-sm">
					{t("ahabUse.nothing")}
				</p>
			) : (
				<ul className="border-border divide-border divide-y border-y text-sm">
					{metrics.ahab.map((group) => (
						<AhabRow key={`${group.kind}:${group.id ?? ""}`} machineId={machineId} group={group} />
					))}
				</ul>
			)}
		</section>
	);
}

function AhabRow({ machineId, group }: { machineId: string; group: AhabUse }) {
	const { t } = useTranslation("machines");
	const processes = t("ahabUse.processes", { count: group.processes });
	const [icon, name, detail, link] = (() => {
		switch (group.kind) {
			case "agent":
				return [
					<AgentIcon key="icon" agent={group.id ?? ""} />,
					agentLabel(group.id ?? "agent"),
					`${t("ahabUse.sessions", { count: group.count })} · ${processes}`,
					<Link
						key="link"
						to="/settings/machines/$id/agents"
						params={{ id: machineId }}
						search={{ agent: group.id ?? undefined }}
						className="after:absolute after:inset-0"
					>
						<span className="sr-only">
							{t("ahabUse.openAgent", { agent: agentLabel(group.id ?? "agent") })}
						</span>
					</Link>,
				];
			case "plugin":
				return [
					<Puzzle key="icon" className="text-muted-foreground size-4" />,
					t("ahabUse.plugin", { plugin: group.id ?? t("ahabUse.unknownPlugin") }),
					processes,
					group.id ? (
						<Link
							key="link"
							to="/settings/plugins/$id"
							params={{ id: group.id }}
							className="after:absolute after:inset-0"
						>
							<span className="sr-only">{t("ahabUse.openPlugin", { plugin: group.id })}</span>
						</Link>
					) : null,
				];
			case "terminal":
				return [
					<SquareTerminal key="icon" className="text-muted-foreground size-4" />,
					t("ahabUse.terminalsTitle"),
					`${t("ahabUse.terminals", { count: group.count })} · ${processes}`,
					null,
				];
			default:
				return [
					<Server key="icon" className="text-muted-foreground size-4" />,
					"Ahab",
					`${t("ahabUse.ahabParts")} · ${processes}`,
					null,
				];
		}
	})();
	return (
		<li
			className={cn(
				"relative grid grid-cols-[1rem_minmax(0,1fr)_4rem_5rem] items-center gap-x-3 px-2 py-2",
				link && "hover:bg-accent/40 transition-colors duration-150",
			)}
		>
			{icon}
			<span className="min-w-0">
				<span className="text-heading block truncate font-medium">{name}</span>
				<span className="text-muted-foreground block truncate text-xs">{detail}</span>
			</span>
			<span className={cn("text-right tabular-nums", text[toneOf(group.cpuPercent)])}>
				{decimal(group.cpuPercent, 1)}%
			</span>
			<span className="text-right tabular-nums">{bytes(group.memoryBytes)}</span>
			{link}
		</li>
	);
}

function Meter({
	label,
	percent,
	tone,
}: {
	label: ReactNode;
	percent: number;
	tone: keyof typeof bar;
}) {
	return (
		<div className="space-y-1">
			<div className="flex items-baseline justify-between gap-3 text-xs">
				<span className="text-muted-foreground min-w-0 truncate">{label}</span>
				<span className={cn("font-semibold tabular-nums", text[tone])}>{Math.round(percent)}%</span>
			</div>
			<span className="bg-muted block h-1.5 overflow-hidden rounded-full">
				<span
					className={cn("block h-full rounded-full transition-[width] duration-500", bar[tone])}
					style={{ width: `${Math.max(0, Math.min(100, percent))}%` }}
				/>
			</span>
		</div>
	);
}

function Pair({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="grid gap-1 py-1.5 sm:grid-cols-[8rem_1fr] sm:gap-4">
			<dt className="text-muted-foreground">{label}</dt>
			<dd>{children}</dd>
		</div>
	);
}

/** The machine's detail, one view at a time, in a panel of its own. */
function Panel({
	tabs,
	initial,
}: {
	tabs: { id: string; label: string; body: ReactNode }[];
	initial: string;
}) {
	const { t } = useTranslation("machines");
	const [open, setOpen] = useState(initial);
	const current = tabs.find((tab) => tab.id === open) ?? tabs[0];
	return (
		<section aria-label={t("resources.tabs.label")} className="space-y-3">
			<div
				role="tablist"
				aria-label={t("resources.tabs.label")}
				className="border-border flex gap-1 overflow-x-auto border-b"
			>
				{tabs.map((tab) => (
					<button
						key={tab.id}
						type="button"
						role="tab"
						aria-selected={tab.id === current?.id}
						onClick={() => setOpen(tab.id)}
						className={cn(
							"-mb-px border-b-2 px-2.5 py-1.5 text-sm font-medium whitespace-nowrap transition-colors duration-150",
							tab.id === current?.id
								? "border-primary text-heading"
								: "text-muted-foreground hover:text-foreground border-transparent",
						)}
					>
						{tab.label}
					</button>
				))}
			</div>
			<div role="tabpanel">{current?.body}</div>
		</section>
	);
}
