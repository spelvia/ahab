import { cn } from "cn";
import { ChevronDown, ChevronRight, RotateCw, Search } from "lucide-react";
import { type ComponentType, type ReactNode, useState } from "react";

import {
	useAgents,
	useAgentUpdates,
	useCheckAgentUpdates,
	useRefreshAgents,
} from "@/modules/machines/agents/api";
import {
	AddCustomAgent,
	AvailableAgents,
	RemoveAgentButton,
} from "@/modules/machines/agents/components/agent-catalog";
import { AgentIcon } from "@/modules/machines/agents/components/agent-icon";
import { AgentUpdateLine, UpdateButton } from "@/modules/machines/agents/components/agent-update";
import {
	type AgentView,
	type Facet,
	FACETS,
	inFacet,
	installed,
	matches,
	sections,
	signInCommand,
	stateOf,
	tightest,
	updatable,
} from "@/modules/machines/agents/model";
import {
	METERED,
	toneOf,
	UsageWindows,
	useAgentUsageOn,
	useRefreshAgentUsage,
} from "@/modules/machines/usage";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatDate, Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";
import { Drawer, DrawerContent } from "@/shared/ui/drawer";

type ModelsList = ComponentType<{ machineId: string; agent: string }>;

/** A time in full, as `toLocaleString` writes it. */
const FULL: Intl.DateTimeFormatOptions = {
	year: "numeric",
	month: "numeric",
	day: "numeric",
	hour: "numeric",
	minute: "numeric",
	second: "numeric",
};

/**
 * The agents on a machine as one list made for many: searchable, filtered
 * by state, what needs you first, then the rest installed, then the agents
 * not installed here (folded). Each row says the essentials; the drawer
 * holds the rest (version and update, program, account, plan limits,
 * models).
 */
export function MachineAgents({
	machine,
	Models,
	selected,
	onSelect,
}: {
	machine: Machine;
	/** The models an agent offers on this machine (sessions knows them). */
	Models?: ModelsList;
	/** The agent whose drawer is open. */
	selected?: string;
	onSelect: (agent: string | undefined) => void;
}) {
	const { t } = useTranslation("machines");
	const agents = useAgents(machine.id);
	const refresh = useRefreshAgents(machine.id);
	const items = agents.data?.items ?? [];
	const metered = items.some((item) => item.state === "ready" && METERED.has(item.agent));
	const usage = useAgentUsageOn(machine.id, metered);
	const refreshUsage = useRefreshAgentUsage(machine.id);
	const updates = useAgentUpdates(
		machine.id,
		items.some((item) => item.state !== "notInstalled"),
	);
	const checkUpdates = useCheckAgentUpdates(machine.id);
	const [query, setQuery] = useState("");
	const [facet, setFacet] = useState<Facet>("all");

	const views: AgentView[] = items.map((status) => ({
		status,
		update: updates.data?.items.find((item) => item.agent === status.agent),
		usage:
			status.state === "ready"
				? usage.data?.items.find((item) => item.agent === status.agent)
				: undefined,
	}));
	const shown = views.filter((view) => inFacet(view, facet) && matches(view, query));
	const grouped = sections(shown);
	const open = views.find((view) => view.status.agent === selected);
	const checkedAt = items.reduce<string | undefined>(
		(latest, item) => (!latest || item.checkedAt > latest ? item.checkedAt : latest),
		undefined,
	);

	return (
		<div className="space-y-4">
			<div className="flex flex-wrap items-center gap-2">
				<label className="border-border focus-within:ring-ring flex h-8 min-w-48 flex-1 items-center gap-2 rounded-md border px-2.5 focus-within:ring-2 sm:max-w-72">
					<Search className="text-muted-foreground size-3.5" />
					<span className="sr-only">{t("agents.search")}</span>
					<input
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("agents.search")}
						className="placeholder:text-muted-foreground min-w-0 flex-1 bg-transparent text-sm outline-none"
					/>
				</label>
				<span className="text-muted-foreground ml-auto text-xs">
					{checkedAt ? (
						<span title={formatDate(checkedAt, FULL)}>
							{t("agents.checked", { when: agoInWords(checkedAt) })}
						</span>
					) : (
						t("agents.notChecked")
					)}
				</span>
				<Button
					variant="ghost"
					size="xs"
					disabled={!machine.online || refresh.isPending}
					title={machine.online ? undefined : t("agents.offline")}
					onClick={() => {
						refresh.mutate({ id: machine.id });
						if (metered) refreshUsage.mutate();
						void checkUpdates.check();
					}}
				>
					<RotateCw className={refresh.isPending ? "animate-spin" : undefined} />
					{refresh.isPending ? t("agents.checking") : t("agents.checkAgain")}
				</Button>
				<AddCustomAgent machine={machine} />
			</div>

			<div className="flex flex-wrap gap-1" role="group" aria-label={t("agents.facetsLabel")}>
				{FACETS.map((id) => {
					const count = views.filter((view) => inFacet(view, id)).length;
					return (
						<button
							key={id}
							type="button"
							aria-pressed={facet === id}
							onClick={() => setFacet(id)}
							className={cn(
								"rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors duration-150",
								facet === id
									? "border-primary/50 bg-primary/12 text-heading"
									: "border-border text-muted-foreground hover:text-foreground",
							)}
						>
							{t(`facets.${id}`)} <span className="tabular-nums opacity-70">{count}</span>
						</button>
					);
				})}
			</div>

			{refresh.isError && (
				<p role="alert" className="text-destructive text-xs">
					{t("agents.checkFailed", { message: refresh.error.message })}
				</p>
			)}
			{agents.isPending && (
				<p className="text-muted-foreground py-2 text-sm" role="status">
					{t("agents.loading")}
				</p>
			)}
			{agents.isError && (
				<p role="alert" className="text-destructive py-2 text-sm">
					{t("agents.loadFailed", { message: agents.error.message })}
				</p>
			)}
			{agents.data && items.length === 0 && (
				<p className="text-muted-foreground py-2 text-sm">
					{machine.online ? t("agents.notDetected") : t("agents.notDetectedOffline")}
				</p>
			)}
			{agents.data && items.length > 0 && shown.length === 0 && (
				<p className="border-border text-muted-foreground border-y py-3 text-sm">
					{query ? t("agents.noMatchQuery", { query }) : t("agents.noMatchFilter")}
				</p>
			)}

			<Group
				title={t("agents.groups.attention")}
				views={grouped.attention}
				machine={machine}
				onSelect={onSelect}
			/>
			<Group
				title={t("agents.groups.installed")}
				views={grouped.installed}
				machine={machine}
				onSelect={onSelect}
			/>
			<Group
				title={t("agents.groups.missing")}
				views={grouped.missing}
				machine={machine}
				onSelect={onSelect}
				folded={facet !== "missing" && !query}
			/>
			{(facet === "all" || facet === "missing") && (
				<AvailableAgents
					machine={machine}
					present={new Set(items.map((item) => item.agent))}
					query={query}
					folded={facet !== "missing" && !query}
				/>
			)}

			<Drawer open={Boolean(open)} onOpenChange={(next) => !next && onSelect(undefined)}>
				{open && (
					<DrawerContent
						title={open.status.label}
						description={stateOf(open.status.state).word}
						header={<AgentIcon agent={open.status.agent} className="mt-0.5 size-5" />}
					>
						<Details
							view={open}
							machine={machine}
							Models={Models}
							onRemoved={() => onSelect(undefined)}
						/>
					</DrawerContent>
				)}
			</Drawer>
		</div>
	);
}

/** One section of the list, with its count; foldable. */
function Group({
	title,
	views,
	machine,
	onSelect,
	folded = false,
}: {
	title: string;
	views: AgentView[];
	machine: Machine;
	onSelect: (agent: string) => void;
	folded?: boolean;
}) {
	const [open, setOpen] = useState<boolean | undefined>(undefined);
	if (views.length === 0) return null;
	const shown = open ?? !folded;
	return (
		<section aria-label={title} className="space-y-1">
			<h3>
				<button
					type="button"
					aria-expanded={shown}
					onClick={() => setOpen(!shown)}
					className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-xs font-medium tracking-wide uppercase"
				>
					{shown ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
					{title} <span className="tabular-nums">({views.length})</span>
				</button>
			</h3>
			{shown && (
				<ul className="border-border divide-border divide-y border-y">
					{views.map((view) => (
						<Row
							key={view.status.agent}
							view={view}
							machine={machine}
							onOpen={() => onSelect(view.status.agent)}
						/>
					))}
				</ul>
			)}
		</section>
	);
}

const DOT: Record<string, string> = {
	ready: "bg-success",
	notSignedIn: "bg-warning",
	unsupportedVersion: "bg-warning",
	error: "bg-destructive",
	notInstalled: "border border-muted-foreground/50",
};

/** One agent in a line: its mark, name, version, state, plan, tightest limit and one action. */
function Row({ view, machine, onOpen }: { view: AgentView; machine: Machine; onOpen: () => void }) {
	const { t } = useTranslation("machines");
	const { status } = view;
	const limit = tightest(view);
	const present = installed(view);
	return (
		<li className="hover:bg-accent/40 relative flex min-h-11 items-center gap-3 px-2 py-1.5 text-sm transition-colors duration-150">
			<AgentIcon agent={status.agent} className={present ? undefined : "opacity-50 grayscale"} />
			<button
				type="button"
				onClick={onOpen}
				className="min-w-0 flex-1 text-left outline-none after:absolute after:inset-0 focus-visible:underline"
				aria-label={t("agents.rowLabel", { agent: status.label })}
			>
				<span className="flex min-w-0 flex-wrap items-baseline gap-x-2">
					<span className={present ? "text-heading font-medium" : "text-muted-foreground"}>
						{status.label}
					</span>
					{status.version && (
						<span className="text-muted-foreground font-mono text-xs">{status.version}</span>
					)}
				</span>
			</button>
			<span className="text-muted-foreground hidden min-w-0 items-center gap-1.5 text-xs sm:flex">
				<span className={cn("size-2 shrink-0 rounded-full", DOT[status.state])} />
				{stateOf(status.state).word}
				{view.usage?.plan && <span>· {view.usage.plan}</span>}
			</span>
			{limit && (
				<span
					className="hidden w-24 items-center gap-1.5 text-xs tabular-nums md:flex"
					title={t("agents.limitTitle", {
						limit: limit.label,
						percent: Math.round(limit.usedPercent),
					})}
				>
					<span className="bg-muted h-1 flex-1 overflow-hidden rounded-full">
						<span
							className={cn(
								"block h-full rounded-full",
								toneOf(limit.usedPercent) === "danger"
									? "bg-destructive"
									: toneOf(limit.usedPercent) === "warning"
										? "bg-warning"
										: "bg-primary",
							)}
							style={{ width: `${Math.min(100, limit.usedPercent)}%` }}
						/>
					</span>
					<span className="text-muted-foreground w-8 text-right">
						{Math.round(limit.usedPercent)}%
					</span>
				</span>
			)}
			<span className="relative z-10 flex items-center gap-1.5">
				{view.update && updatable(view) && view.update.canUpdate && (
					<UpdateButton machine={machine} update={view.update} />
				)}
				{status.state === "notSignedIn" && (
					<span className="pill pill-warn">{t("agents.signIn")}</span>
				)}
			</span>
			<ChevronRight className="text-muted-foreground size-4 shrink-0" />
		</li>
	);
}

/** Everything about one agent on the machine, in its drawer. */
function Details({
	view,
	machine,
	Models,
	onRemoved,
}: {
	view: AgentView;
	machine: Machine;
	Models?: ModelsList;
	onRemoved: () => void;
}) {
	const { t } = useTranslation("machines");
	const { status, update, usage } = view;
	const added = status.agent.startsWith("acp:") || status.agent.startsWith("custom:");
	const command = status.state === "notSignedIn" ? signInCommand(status.agent) : undefined;
	return (
		<div className="space-y-6 text-sm">
			{status.state !== "ready" && status.state !== "notInstalled" && (
				<p className="border-warning/45 bg-warning/10 rounded-md border px-3 py-2">
					{command ? (
						<Trans
							t={t}
							i18nKey="agentDetails.signIn"
							values={{ command, machine: machine.name }}
							components={{ code: <code className="font-mono text-xs" /> }}
						/>
					) : (
						(status.detail ?? stateOf(status.state).word)
					)}
				</p>
			)}
			{status.state === "notInstalled" && (
				<p className="text-muted-foreground">
					{status.detail
						? t("agentDetails.notInstalledBecause", {
								agent: status.label,
								machine: machine.name,
								detail: status.detail,
							})
						: t("agentDetails.notInstalled", { agent: status.label, machine: machine.name })}
				</p>
			)}
			{status.state === "ready" && status.detail && (
				<p className="border-border bg-muted/40 rounded-md border px-3 py-2">{status.detail}</p>
			)}
			{added && (
				<p className="flex flex-wrap items-center gap-2">
					<span className="text-muted-foreground text-xs">
						{status.agent.startsWith("custom:")
							? t("agentDetails.custom")
							: t("agentDetails.registry")}
					</span>
					<RemoveAgentButton
						machine={machine}
						agent={status.agent}
						name={status.label}
						onRemoved={onRemoved}
					/>
				</p>
			)}

			{installed(view) && (
				<DetailSection title={t("agentDetails.about")}>
					<dl className="divide-border divide-y">
						<Item label={t("agentDetails.version")}>
							{update ? (
								<AgentUpdateLine machine={machine} update={update} />
							) : (
								<span className="font-mono text-xs">
									{status.version ?? t("agentDetails.unknownVersion")}
								</span>
							)}
						</Item>
						{status.path && (
							<Item label={t("agentDetails.program")}>
								<span className="flex min-w-0 items-center gap-1">
									<span className="truncate font-mono text-xs" title={status.path}>
										{status.path}
									</span>
									<CopyButton text={status.path} label={t("agentDetails.pathCopy")} />
								</span>
							</Item>
						)}
						{(status.account || usage?.plan) && (
							<Item label={t("agentDetails.account")}>
								{[status.account, usage?.plan].filter(Boolean).join(" · ")}
							</Item>
						)}
						<Item label={t("agentDetails.checked")}>{agoInWords(status.checkedAt)}</Item>
					</dl>
				</DetailSection>
			)}

			{usage && (
				<DetailSection title={t("agentDetails.planLimits")}>
					<UsageWindows usage={usage} />
				</DetailSection>
			)}

			{status.state === "ready" && Models && (
				<DetailSection title={t("agentDetails.models")}>
					<Models machineId={machine.id} agent={status.agent} />
				</DetailSection>
			)}
		</div>
	);
}

function DetailSection({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="space-y-2">
			<h3 className="text-muted-foreground text-xs font-medium tracking-wide uppercase">{title}</h3>
			{children}
		</section>
	);
}

function Item({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="grid gap-1 py-2 sm:grid-cols-[6rem_minmax(0,1fr)] sm:gap-3">
			<dt className="text-muted-foreground">{label}</dt>
			<dd className="min-w-0">{children}</dd>
		</div>
	);
}
