import { cn } from "cn";
import { Bot, ChevronDown, ChevronRight, SquareTerminal } from "lucide-react";
import { type ReactNode, useState } from "react";

import { shellSummary, unwrapShell } from "@/modules/sessions/chat/components/command-text";
import {
	active,
	agentName,
	type ObservedAgent,
	type ObservedTask,
	type StopState,
} from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";

/**
 * Above the composer, one line per kind of side work the chat's agent runs
 * (Stubb's observation): its subagents, and its background tasks. Each says
 * how many are at work and opens to the list; finished ones stay, dimmed,
 * until the chat stops.
 */
function Strip({
	icon,
	summary,
	busy,
	children,
}: {
	icon: ReactNode;
	summary: string;
	busy: boolean;
	children: ReactNode;
}) {
	const [open, setOpen] = useState<boolean>();
	const expanded = open ?? busy;
	return (
		<div className="border-border bg-card/60 rounded-lg border text-xs">
			<button
				type="button"
				aria-expanded={expanded}
				onClick={() => setOpen(!expanded)}
				className="text-muted-foreground hover:text-foreground flex w-full items-center gap-2 px-3 py-1.5 text-left"
			>
				{icon}
				<span className="min-w-0 flex-1 truncate">{summary}</span>
				{busy ? (
					<span
						className="bg-success size-1.5 shrink-0 rounded-full motion-safe:animate-pulse"
						aria-hidden
					/>
				) : null}
				<ChevronDown
					className={cn("size-3.5 shrink-0 transition-transform", expanded && "rotate-180")}
				/>
			</button>
			{expanded ? (
				<ul className="border-border max-h-56 divide-y overflow-auto border-t">{children}</ul>
			) : null}
		</div>
	);
}

/** The subagents of the agent shown; choosing one shows its conversation. */
export function SubagentStrip({
	agents,
	onOpen,
	onStop,
	activity,
	stops,
}: {
	activity?: (id: string) => string | undefined;
	/** Stops one; `name` says which, for what it reports. */
	onStop?: (id: string, name: string) => void;
	/** Where stopping each stands, by ID. */
	stops?: Readonly<Record<string, StopState>>;
	agents: readonly ObservedAgent[];
	onOpen: (agent: ObservedAgent) => void;
}) {
	const { t } = useTranslation("sessions");
	const [history, setHistory] = useState(false);
	if (agents.length === 0) return null;
	const busy = agents.filter((agent) => active(agent.status)).length;
	return (
		<Strip
			icon={<Bot className="size-3.5 shrink-0" aria-hidden />}
			summary={
				busy
					? t("activity.agents", { count: busy })
					: t("activity.agentsDone", { count: agents.length })
			}
			busy={busy > 0}
		>
			{agents
				.filter((item) => !busy || history || active(item.status))
				.map((agent) => (
					<li key={agent.id} className="flex flex-wrap items-center">
						<button
							type="button"
							onClick={() => onOpen(agent)}
							title={t("activity.openAgent", { name: agentName(agent) })}
							className={cn(
								"hover:bg-accent flex min-w-0 flex-1 items-start gap-2 px-3 py-1.5 text-left",
								!active(agent.status) && "opacity-60",
							)}
						>
							<Bot className="text-muted-foreground mt-0.5 size-3.5 shrink-0" aria-hidden />
							<span className="min-w-0 flex-1">
								<span className="text-heading">{agentName(agent)}</span>
								{agent.model ? (
									<span className="text-muted-foreground"> · {agent.model}</span>
								) : null}
								{activity?.(agent.id) || agent.description ? (
									<span className="text-muted-foreground block truncate">
										{activity?.(agent.id) || agent.description}
									</span>
								) : null}
							</span>
							<Status status={agent.status} />
							<ChevronRight
								className="text-muted-foreground mt-0.5 size-3.5 shrink-0"
								aria-hidden
							/>
						</button>
						{onStop && active(agent.status) && (
							<StopButton
								label={t("activity.stop.agent", { name: agentName(agent) })}
								state={stops?.[agent.id]}
								stopping={agent.status === "stopping"}
								onStop={() => onStop(agent.id, agentName(agent))}
							/>
						)}
						<StopNotice state={stops?.[agent.id]} name={agentName(agent)} />
					</li>
				))}
			{busy > 0 && agents.length > busy && (
				<li>
					<button
						type="button"
						className="text-muted-foreground px-3 py-2"
						onClick={() => setHistory(!history)}
					>
						{t(history ? "tools.hideFinished" : "tools.showFinished", {
							count: agents.length - busy,
						})}
					</button>
				</li>
			)}
		</Strip>
	);
}

/** The agent's tasks, such as background commands; choosing one shows its output. */
export function TaskStrip({
	tasks,
	onOpen,
	onStop,
	stops,
}: {
	/** Stops one; `name` says which, for what it reports. */
	onStop?: (id: string, name: string) => void;
	/** Where stopping each stands, by ID. */
	stops?: Readonly<Record<string, StopState>>;
	tasks: readonly ObservedTask[];
	onOpen: (task: ObservedTask) => void;
}) {
	const { t } = useTranslation("sessions");
	const [history, setHistory] = useState(false);
	if (tasks.length === 0) return null;
	const busy = tasks.filter((task) => active(task.status)).length;
	return (
		<Strip
			icon={<SquareTerminal className="size-3.5 shrink-0" aria-hidden />}
			summary={
				busy
					? t("activity.tasks", { count: busy })
					: t("activity.tasksDone", { count: tasks.length })
			}
			busy={busy > 0}
		>
			{tasks
				.filter((item) => !busy || history || active(item.status))
				.map((task) => (
					<li key={task.id} className="flex flex-wrap items-center">
						<button
							type="button"
							onClick={() => onOpen(task)}
							title={
								task.kind.type === "command"
									? unwrapShell(task.kind.command).script
									: t("activity.openTask")
							}
							className={cn(
								"hover:bg-accent flex min-w-0 flex-1 items-start gap-2 px-3 py-1.5 text-left",
								!active(task.status) && "opacity-60",
							)}
						>
							<SquareTerminal
								className="text-muted-foreground mt-0.5 size-3.5 shrink-0"
								aria-hidden
							/>
							<span className="min-w-0 flex-1">
								<span className={cn("block truncate", task.kind.type === "command" && "font-mono")}>
									{task.kind.type === "command" ? shellSummary(task.kind.command) : taskWhat(task)}
								</span>
								{task.description && task.kind.type === "command" ? (
									<span className="text-muted-foreground block truncate">{task.description}</span>
								) : null}
							</span>
							{task.background ? (
								<span className="text-muted-foreground border-border shrink-0 rounded border px-1">
									{t("activity.background")}
								</span>
							) : null}
							<Status status={task.status} />
						</button>
						{onStop && active(task.status) && (
							<StopButton
								label={t("activity.stop.task", { name: shortName(taskWhat(task)) })}
								state={stops?.[task.id]}
								stopping={task.status === "stopping"}
								onStop={() => onStop(task.id, shortName(taskWhat(task)))}
							/>
						)}
						<StopNotice state={stops?.[task.id]} name={shortName(taskWhat(task))} />
					</li>
				))}
			{busy > 0 && tasks.length > busy && (
				<li>
					<button
						type="button"
						className="text-muted-foreground px-3 py-2"
						onClick={() => setHistory(!history)}
					>
						{t(history ? "tools.hideFinished" : "tools.showFinished", {
							count: tasks.length - busy,
						})}
					</button>
				</li>
			)}
		</Strip>
	);
}

/** A name short enough to say: a long command is cut. */
function shortName(name: string): string {
	const flat = name.replace(/\s+/g, " ").trim();
	return flat.length > 60 ? `${flat.slice(0, 59)}…` : flat;
}

/** A row's Stop: named for what it stops; "Stopping…" while it stops. */
function StopButton({
	label,
	state,
	stopping,
	onStop,
}: {
	label: string;
	state: StopState | undefined;
	/** The agent reports it stopping. */
	stopping: boolean;
	onStop: () => void;
}) {
	const { t } = useTranslation("sessions");
	const busy = stopping || state?.phase === "stopping";
	return (
		<button
			type="button"
			aria-label={busy ? t("activity.stop.busy", { name: label }) : label}
			title={label}
			className="text-muted-foreground hover:text-destructive focus-visible:ring-ring min-h-11 shrink-0 rounded-md px-3 outline-none focus-visible:ring-2 disabled:opacity-60 sm:min-h-8"
			disabled={busy}
			onClick={onStop}
		>
			{t(busy ? "tools.stopping" : "composer.stop")}
		</button>
	);
}

/** Under a row: why it did not stop, or that the agent could not confirm it. */
function StopNotice({ state, name }: { state: StopState | undefined; name: string }) {
	const { t } = useTranslation("sessions");
	if (!state || (state.phase === "stopping" && !state.unconfirmed)) return null;
	if (state.phase === "failed")
		return (
			<p role="alert" className="text-destructive w-full px-3 pb-1.5">
				{state.message
					? t("activity.stop.rejectedBecause", { name, message: state.message })
					: t("activity.stop.rejected", { name })}
			</p>
		);
	return (
		<p role="status" className="text-muted-foreground w-full px-3 pb-1.5">
			{t(state.phase === "timedOut" ? "activity.stop.timedOut" : "activity.stop.unconfirmed", {
				name,
			})}
		</p>
	);
}

/** What a task runs: its command, its name, else what the agent said of it. */
export function taskWhat(task: ObservedTask): string {
	return task.kind.type === "command"
		? task.kind.command
		: task.kind.type === "other"
			? task.kind.name
			: (task.description ?? "");
}

const KNOWN = new Set([
	"pending",
	"running",
	"waiting",
	"paused",
	"stopping",
	"completed",
	"failed",
	"interrupted",
	"stopped",
	"closed",
]);

export function Status({ status }: { status: string }) {
	const { t } = useTranslation("sessions");
	return (
		<span
			className={cn(
				"shrink-0",
				status === "failed"
					? "text-destructive"
					: active(status)
						? "text-success"
						: "text-muted-foreground",
			)}
		>
			{KNOWN.has(status) ? t(`activity.status.${status as "running"}`) : status}
		</span>
	);
}
