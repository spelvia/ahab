import { cn } from "cn";
import { ChevronDown, ChevronRight, Search } from "lucide-react";
import { type ReactNode, useState } from "react";

import { agentLabel } from "@/modules/machines/agents";
import { bytes, cpuTime, decimal, duration, toneOf } from "@/modules/machines/metrics/model";
import type {
	AppMetrics,
	MachineMetrics,
	ProcessMetrics,
} from "@/shared/api/generated/ahab.schemas";
import { formatDate, t as translate, Trans, useTranslation } from "@/shared/i18n";
import { Drawer, DrawerContent } from "@/shared/ui/drawer";

/** A time in full, as `toLocaleString` writes it. */
const FULL: Intl.DateTimeFormatOptions = {
	year: "numeric",
	month: "numeric",
	day: "numeric",
	hour: "numeric",
	minute: "numeric",
	second: "numeric",
};

const text = {
	ok: "text-foreground",
	warning: "text-warning",
	danger: "text-destructive",
} as const;

type View = "apps" | "tree" | "flat";
type Order = "cpu" | "memory" | "time" | "io";

const ORDER: Record<Order, (process: Sortable) => number> = {
	cpu: (process) => process.cpuPercent,
	memory: (process) => process.memoryBytes,
	time: (process) => process.cpuTimeMs ?? 0,
	io: (process) => (process.readPerSecond ?? 0) + (process.writtenPerSecond ?? 0),
};

type Sortable = Pick<ProcessMetrics, "cpuPercent" | "memoryBytes"> &
	Partial<Pick<ProcessMetrics, "cpuTimeMs" | "readPerSecond" | "writtenPerSecond">>;

/** A process's state as a word, and whether it needs a look. */
function state(status: string): { word: string; tone: keyof typeof text } {
	switch (status) {
		case "Run":
			return { word: translate("processes.state.running", { ns: "machines" }), tone: "ok" };
		case "Sleep":
		case "Idle":
			return { word: translate("processes.state.sleeping", { ns: "machines" }), tone: "ok" };
		case "Zombie":
			return { word: translate("processes.state.zombie", { ns: "machines" }), tone: "danger" };
		case "Stop":
		case "Tracing":
			return { word: translate("processes.state.stopped", { ns: "machines" }), tone: "warning" };
		case "UninterruptibleDiskSleep":
			return { word: translate("processes.state.disk", { ns: "machines" }), tone: "warning" };
		default:
			return { word: status, tone: "ok" };
	}
}

/** What Ahab started a process for, in words. */
function roleWord(role: string | null | undefined): string | undefined {
	if (!role) return undefined;
	if (role === "ahab") return "Ahab";
	if (role === "terminal") return translate("processes.role.terminal", { ns: "machines" });
	if (role.startsWith("agent:"))
		return translate("processes.role.agent", { ns: "machines", agent: agentLabel(role.slice(6)) });
	if (role.startsWith("plugin:"))
		return translate("processes.role.plugin", { ns: "machines", plugin: role.slice(7) });
	return role;
}

function rate(bytesPerSecond: number): string {
	return bytesPerSecond > 0
		? translate("units.rate", { ns: "machines", size: bytes(bytesPerSecond) })
		: "–";
}

/**
 * A machine's processes as a scrolling table: by program, as a tree of
 * parents and children, or one by one; sortable by CPU, memory, CPU time
 * or disk; searchable; all or Ahab's. A process opens its details.
 */
export function ProcessExplorer({ metrics }: { metrics: MachineMetrics }) {
	const { t } = useTranslation("machines");
	const [view, setView] = useState<View>("apps");
	const [only, setOnly] = useState<"all" | "ahab">("all");
	const [order, setOrder] = useState<Order>("cpu");
	const [query, setQuery] = useState("");
	const [open, setOpen] = useState<number>();
	const byPid = new Map(metrics.processes.map((process) => [process.pid, process]));
	const wanted = query.trim().toLowerCase();
	const keep = (process: ProcessMetrics) =>
		(only === "all" || process.ahab) &&
		(!wanted ||
			`${process.name} ${process.script ?? ""} ${process.user ?? ""} ${process.pid}`
				.toLowerCase()
				.includes(wanted));
	const sort = <T extends Sortable>(items: T[]) =>
		items.toSorted((a, b) => ORDER[order](b) - ORDER[order](a));

	const apps = sort(
		metrics.apps
			.map((app) => ({
				...app,
				members: sort(
					app.members
						.map((pid) => byPid.get(pid))
						.filter((process): process is ProcessMetrics => Boolean(process))
						.filter(keep),
				),
			}))
			.filter((app) => app.members.length > 0),
	);
	const flat = sort(metrics.processes.filter(keep));
	const selected = open === undefined ? undefined : byPid.get(open);

	return (
		<div className="space-y-2">
			<div className="flex flex-wrap items-center gap-2 text-xs">
				<label className="border-border focus-within:ring-ring flex h-7 items-center gap-1.5 rounded-md border px-2 focus-within:ring-2">
					<Search className="text-muted-foreground size-3.5" />
					<span className="sr-only">{t("processes.search")}</span>
					<input
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("processes.searchPlaceholder")}
						className="placeholder:text-muted-foreground w-32 bg-transparent outline-none"
					/>
				</label>
				<Toggles
					label={t("processes.view.label")}
					value={view}
					options={[
						["apps", t("processes.view.apps")],
						["tree", t("processes.view.tree")],
						["flat", t("processes.view.flat")],
					]}
					onChange={(next) => setView(next as View)}
				/>
				<Toggles
					label={t("processes.show.label")}
					value={only}
					options={[
						["all", t("processes.show.all")],
						["ahab", t("processes.show.ahab")],
					]}
					onChange={(next) => setOnly(next as "all" | "ahab")}
				/>
				<span className="text-muted-foreground ml-auto">
					{t("processes.running", { running: metrics.processCount })}
				</span>
			</div>

			<div className="border-border max-h-[28rem] overflow-auto rounded-lg border">
				<table className="w-full min-w-[40rem] border-collapse text-sm">
					<thead className="bg-background sticky top-0 z-10 shadow-[0_1px_0_var(--border)]">
						<tr className="text-muted-foreground text-left text-xs">
							<th scope="col" className="py-2 pr-3 pl-3 font-medium">
								{view === "apps" ? t("processes.columns.program") : t("processes.columns.process")}
							</th>
							<th scope="col" className="hidden py-2 pr-3 font-medium md:table-cell">
								{t("processes.columns.user")}
							</th>
							<th scope="col" className="hidden py-2 pr-3 font-medium lg:table-cell">
								{t("processes.columns.state")}
							</th>
							<Sort id="cpu" order={order} onSort={setOrder}>
								{t("processes.columns.cpu")}
							</Sort>
							<Sort id="memory" order={order} onSort={setOrder}>
								{t("processes.columns.memory")}
							</Sort>
							<Sort id="time" order={order} onSort={setOrder} className="hidden sm:table-cell">
								{t("processes.columns.cpuTime")}
							</Sort>
							<Sort id="io" order={order} onSort={setOrder} className="hidden md:table-cell">
								{t("processes.columns.disk")}
							</Sort>
						</tr>
					</thead>
					<tbody>
						{view === "apps" &&
							apps.map((app) => <AppRows key={app.name} app={app} onOpen={setOpen} />)}
						{view === "tree" && <Tree processes={flat} onOpen={setOpen} sort={sort} />}
						{view === "flat" &&
							flat.map((process) => (
								<ProcessRow key={process.pid} process={process} onOpen={setOpen} />
							))}
					</tbody>
				</table>
				{(view === "apps" ? apps.length : flat.length) === 0 && (
					<p className="text-muted-foreground px-3 py-4 text-sm">
						{wanted ? t("processes.noMatchQuery", { query }) : t("processes.noMatch")}
					</p>
				)}
			</div>

			<Drawer open={Boolean(selected)} onOpenChange={(next) => !next && setOpen(undefined)}>
				{selected && (
					<DrawerContent
						title={selected.name}
						description={
							selected.script
								? t("processes.drawerWithScript", { pid: selected.pid, script: selected.script })
								: t("processes.drawer", { pid: selected.pid })
						}
					>
						<ProcessDetails
							process={selected}
							parent={selected.parent ? byPid.get(selected.parent) : undefined}
							onOpen={setOpen}
						/>
					</DrawerContent>
				)}
			</Drawer>
		</div>
	);
}

/** A program's processes together, opened to list them. */
function AppRows({
	app,
	onOpen,
}: {
	app: Omit<AppMetrics, "members"> & { members: ProcessMetrics[] };
	onOpen: (pid: number) => void;
}) {
	const { t } = useTranslation("machines");
	const [open, setOpen] = useState(false);
	const single = app.members.length === 1 && app.processes === 1 ? app.members[0] : undefined;
	if (single) return <ProcessRow process={single} onOpen={onOpen} />;
	const ahab = app.members.some((process) => process.ahab);
	const users = [...new Set(app.members.map((process) => process.user).filter(Boolean))];
	const time = app.members.reduce((sum, process) => sum + process.cpuTimeMs, 0);
	const io = app.members.reduce(
		(sum, process) => sum + process.readPerSecond + process.writtenPerSecond,
		0,
	);
	return (
		<>
			<tr className="border-border hover:bg-accent/40 border-b">
				<td className="py-1.5 pr-3 pl-3">
					<button
						type="button"
						aria-expanded={open}
						onClick={() => setOpen(!open)}
						className="flex max-w-full min-w-0 items-center gap-1.5 text-left"
					>
						{open ? (
							<ChevronDown className="text-muted-foreground size-3.5 shrink-0" />
						) : (
							<ChevronRight className="text-muted-foreground size-3.5 shrink-0" />
						)}
						<span className="text-foreground truncate font-medium">{app.name}</span>
						<span className="text-muted-foreground shrink-0 text-xs tabular-nums">
							({app.processes})
						</span>
						{ahab && <span className="pill pill-muted shrink-0">Ahab</span>}
					</button>
				</td>
				<td className="text-muted-foreground hidden truncate py-1.5 pr-3 text-xs md:table-cell">
					{users.join(", ")}
				</td>
				<td className="hidden py-1.5 pr-3 lg:table-cell" />
				<Numbers cpu={app.cpuPercent} memory={app.memoryBytes} time={time} io={io} />
			</tr>
			{open &&
				app.members.map((process) => (
					<ProcessRow key={process.pid} process={process} onOpen={onOpen} depth={1} />
				))}
			{open && app.processes > app.members.length && (
				<tr className="border-border bg-muted/30 border-b">
					<td colSpan={7} className="text-muted-foreground py-1.5 pl-9 text-xs">
						{t("processes.notListed", { more: app.processes - app.members.length })}
					</td>
				</tr>
			)}
		</>
	);
}

/** The processes as parents and children (each under its nearest listed parent). */
function Tree({
	processes,
	onOpen,
	sort,
}: {
	processes: ProcessMetrics[];
	onOpen: (pid: number) => void;
	sort: <T extends Sortable>(items: T[]) => T[];
}) {
	const listed = new Set(processes.map((process) => process.pid));
	const children = new Map<number | undefined, ProcessMetrics[]>();
	for (const process of processes) {
		const parent =
			process.parent !== null && process.parent !== undefined && listed.has(process.parent)
				? process.parent
				: undefined;
		children.set(parent, [...(children.get(parent) ?? []), process]);
	}
	const rows: ReactNode[] = [];
	const walk = (parent: number | undefined, depth: number) => {
		for (const process of sort(children.get(parent) ?? [])) {
			rows.push(<ProcessRow key={process.pid} process={process} onOpen={onOpen} depth={depth} />);
			// Deep chains stay readable.
			if (depth < 12) walk(process.pid, depth + 1);
		}
	};
	walk(undefined, 0);
	return <>{rows}</>;
}

function ProcessRow({
	process,
	onOpen,
	depth = 0,
}: {
	process: ProcessMetrics;
	onOpen: (pid: number) => void;
	depth?: number;
}) {
	const { t } = useTranslation("machines");
	const { word, tone } = state(process.status);
	return (
		<tr
			className={cn(
				"border-border hover:bg-accent/40 cursor-pointer border-b",
				depth > 0 && "bg-muted/20",
			)}
			onClick={() => onOpen(process.pid)}
		>
			<td className="py-1.5 pr-3 pl-3" style={{ paddingLeft: `${0.75 + depth * 1.1}rem` }}>
				<button
					type="button"
					onClick={(event) => {
						event.stopPropagation();
						onOpen(process.pid);
					}}
					className="flex max-w-full min-w-0 items-center gap-2 text-left outline-none focus-visible:underline"
					aria-label={t("processes.rowLabel", { name: process.name, pid: process.pid })}
				>
					{depth > 0 && <span className="text-muted-foreground/60 shrink-0 text-xs">└</span>}
					<span className="text-foreground truncate">{process.name}</span>
					{process.script && (
						<span className="text-muted-foreground truncate font-mono text-xs">
							{process.script}
						</span>
					)}
					<span className="text-muted-foreground shrink-0 font-mono text-[0.6875rem] tabular-nums">
						{process.pid}
					</span>
					{process.ahab && <span className="pill pill-muted shrink-0">Ahab</span>}
				</button>
			</td>
			<td className="text-muted-foreground hidden truncate py-1.5 pr-3 text-xs md:table-cell">
				{process.user ?? "–"}
			</td>
			<td
				className={cn(
					"hidden py-1.5 pr-3 text-xs lg:table-cell",
					tone === "ok" ? "text-muted-foreground" : text[tone],
				)}
			>
				{word}
				{process.threads ? ` · ${t("processes.threads", { threads: process.threads })}` : ""}
			</td>
			<Numbers
				cpu={process.cpuPercent}
				memory={process.memoryBytes}
				time={process.cpuTimeMs}
				io={process.readPerSecond + process.writtenPerSecond}
			/>
		</tr>
	);
}

function Numbers({
	cpu,
	memory,
	time,
	io,
}: {
	cpu: number;
	memory: number;
	time: number;
	io: number;
}) {
	return (
		<>
			<td className={cn("py-1.5 pr-3 text-right tabular-nums", text[toneOf(cpu)])}>
				{decimal(cpu, 1)}%
			</td>
			<td className="text-foreground py-1.5 pr-3 text-right tabular-nums">{bytes(memory)}</td>
			<td className="text-muted-foreground hidden py-1.5 pr-3 text-right text-xs tabular-nums sm:table-cell">
				{cpuTime(time)}
			</td>
			<td className="text-muted-foreground hidden py-1.5 pr-3 text-right text-xs tabular-nums md:table-cell">
				{rate(io)}
			</td>
		</>
	);
}

/** Everything known about one process. */
function ProcessDetails({
	process,
	parent,
	onOpen,
}: {
	process: ProcessMetrics;
	parent?: ProcessMetrics;
	onOpen: (pid: number) => void;
}) {
	const { t } = useTranslation("machines");
	const { word, tone } = state(process.status);
	const role = roleWord(process.role);
	return (
		<dl className="divide-border divide-y text-sm">
			{role && <Item label={t("processes.details.startedBy")}>{role}</Item>}
			<Item label={t("processes.columns.state")}>
				<span className={tone === "ok" ? undefined : text[tone]}>{word}</span>
				{process.threads ? ` · ${t("processes.threads", { threads: process.threads })}` : ""}
			</Item>
			<Item label={t("processes.columns.user")}>{process.user ?? "–"}</Item>
			<Item label={t("processes.details.parent")}>
				{parent ? (
					<button
						type="button"
						onClick={() => onOpen(parent.pid)}
						className="underline underline-offset-4"
					>
						{parent.name} {parent.pid}
					</button>
				) : (
					(process.parent ?? "–")
				)}
			</Item>
			<Item label={t("processes.columns.cpu")}>
				<Trans
					t={t}
					i18nKey="processes.details.cpu"
					values={{ percent: decimal(process.cpuPercent, 1), time: cpuTime(process.cpuTimeMs) }}
					components={{ cpu: <span className={text[toneOf(process.cpuPercent)]} /> }}
				/>
			</Item>
			<Item label={t("processes.columns.memory")}>
				{t("processes.details.memory", {
					resident: bytes(process.memoryBytes),
					virtual: bytes(process.virtualBytes),
				})}
			</Item>
			<Item label={t("processes.columns.disk")}>
				{t("processes.details.disk", {
					read: rate(process.readPerSecond),
					written: rate(process.writtenPerSecond),
				})}
			</Item>
			<Item label={t("processes.details.runningFor")}>
				{duration(process.runSeconds)}
				{process.startedAt && (
					<span className="text-muted-foreground">
						{" "}
						· {t("processes.details.since", { date: formatDate(process.startedAt, FULL) })}
					</span>
				)}
			</Item>
			<Item label={t("processes.details.program")}>
				<span className="font-mono text-xs break-all">{process.executable ?? "–"}</span>
			</Item>
			<Item label={t("processes.details.folder")}>
				<span className="font-mono text-xs break-all">{process.cwd ?? "–"}</span>
			</Item>
		</dl>
	);
}

function Item({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="grid gap-1 py-2 sm:grid-cols-[7rem_minmax(0,1fr)] sm:gap-3">
			<dt className="text-muted-foreground">{label}</dt>
			<dd className="min-w-0">{children}</dd>
		</div>
	);
}

function Toggles({
	label,
	value,
	options,
	onChange,
}: {
	label: string;
	value: string;
	options: [string, string][];
	onChange: (value: string) => void;
}) {
	return (
		<div className="border-border flex rounded-md border p-0.5" role="group" aria-label={label}>
			{options.map(([id, name]) => (
				<button
					key={id}
					type="button"
					aria-pressed={value === id}
					onClick={() => onChange(id)}
					className={cn(
						"rounded px-2 py-0.5 font-medium transition-colors duration-150",
						value === id ? "bg-accent text-heading" : "text-muted-foreground hover:text-foreground",
					)}
				>
					{name}
				</button>
			))}
		</div>
	);
}

function Sort({
	id,
	order,
	onSort,
	className,
	children,
}: {
	id: Order;
	order: Order;
	onSort: (order: Order) => void;
	className?: string;
	children: ReactNode;
}) {
	const active = order === id;
	return (
		<th
			scope="col"
			className={cn("w-24 py-2 pr-3 text-right font-medium", className)}
			aria-sort={active ? "descending" : "none"}
		>
			<button
				type="button"
				onClick={() => onSort(id)}
				className={cn("hover:text-foreground", active && "text-heading")}
			>
				{children}
				{active ? " ↓" : ""}
			</button>
		</th>
	);
}
