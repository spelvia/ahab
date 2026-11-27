import { cn } from "cn";
import { useState } from "react";

import {
	useLanguageServerLog,
	useLanguageServers,
	useRestartLanguageServer,
} from "@/modules/tools/lsp";
import { machineProblem, uptime } from "@/modules/tools/registry";
import { problemCode } from "@/shared/api/fetch";
import type { LanguageServerDto, Machine } from "@/shared/api/generated/ahab.schemas";
import { byteSize } from "@/shared/format";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { toastError } from "@/shared/ui/toast";

const DOT: Record<string, string> = {
	running: "bg-success",
	starting: "bg-warning",
	failed: "bg-destructive",
	stopped: "border border-muted-foreground/50",
};

/** The language servers running on a machine: their root, state, memory and uptime. */
export function LanguageServers({ machine }: { machine: Machine }) {
	const { t } = useTranslation("tools");
	const servers = useLanguageServers(machine.id, machine.online);
	const restart = useRestartLanguageServer(machine.id);
	const [log, setLog] = useState<LanguageServerDto>();
	const items = servers.data?.items ?? [];

	return (
		<div className="space-y-2">
			{servers.isError && (
				<p role="alert" className="text-destructive text-sm">
					{machineProblem(problemCode(servers.error), servers.error.message)}
				</p>
			)}
			{servers.data?.memoryBudget ? (
				<MemoryUse
					used={servers.data.memory ?? 0}
					budget={servers.data.memoryBudget}
					machine={servers.data.machineMemory ?? null}
				/>
			) : null}
			{servers.data && items.length === 0 && (
				<p className="border-border text-muted-foreground border-y py-3 text-sm">
					{t("languageServers.none")}
				</p>
			)}
			{items.length > 0 && (
				<ul className="border-border divide-border divide-y border-y text-sm">
					{items.map((server) => (
						<li key={server.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 py-2">
							<span
								className={cn("size-2 shrink-0 rounded-full", DOT[server.state] ?? DOT.stopped)}
							/>
							<span className="text-heading font-medium">{server.name}</span>
							<span
								className="text-muted-foreground min-w-0 flex-1 truncate font-mono text-xs"
								title={server.root}
							>
								{server.root}
							</span>
							<span className="text-muted-foreground text-xs tabular-nums">
								{[
									server.state,
									server.memory != null && byteSize(server.memory),
									server.state === "running" &&
										t("languageServers.up", { time: uptime(server.startedAt) }),
									t("languageServers.documents", { count: server.documents }),
								]
									.filter(Boolean)
									.join(" · ")}
							</span>
							<span className="flex gap-1">
								<Button
									size="xs"
									variant="ghost"
									disabled={restart.isPending || !machine.online}
									onClick={() =>
										restart.mutate({ id: machine.id, server: server.id }, { onError: toastError })
									}
								>
									{t("languageServers.restart")}
								</Button>
								<Button size="xs" variant="ghost" onClick={() => setLog(server)}>
									{t("languageServers.logs")}
								</Button>
							</span>
							{server.detail && (
								<p
									className={cn(
										"w-full pl-5 text-xs",
										server.state === "failed" ? "text-destructive" : "text-muted-foreground",
									)}
								>
									{server.detail}
								</p>
							)}
						</li>
					))}
				</ul>
			)}
			<Dialog open={Boolean(log)} onOpenChange={(open) => !open && setLog(undefined)}>
				{log && <Log machineId={machine.id} server={log} />}
			</Dialog>
		</div>
	);
}

/** What the servers take together against the machine's memory budget. */
function MemoryUse({
	used,
	budget,
	machine,
}: {
	used: number;
	budget: number;
	machine: number | null;
}) {
	const { t } = useTranslation("tools");
	const share = Math.min(1, used / budget);
	return (
		<div className="space-y-1 text-xs">
			<p className="text-muted-foreground">
				<Trans
					t={t}
					i18nKey={machine ? "languageServers.memoryOfMachine" : "languageServers.memory"}
					values={{
						used: byteSize(used),
						budget: byteSize(budget),
						machine: machine ? byteSize(machine) : "",
					}}
					components={{
						used: <span className="text-foreground tabular-nums" />,
						budget: <span className="tabular-nums" />,
					}}
				/>
			</p>
			<div
				role="meter"
				aria-label={t("languageServers.memoryMeter")}
				aria-valuemin={0}
				aria-valuemax={budget}
				aria-valuenow={Math.min(used, budget)}
				className="bg-muted h-1.5 overflow-hidden rounded-full"
			>
				<div
					className={cn("h-full rounded-full", share > 0.9 ? "bg-warning" : "bg-primary")}
					style={{ width: `${share * 100}%` }}
				/>
			</div>
			{used > budget && <p className="text-warning">{t("languageServers.overBudget")}</p>}
		</div>
	);
}

function Log({ machineId, server }: { machineId: string; server: LanguageServerDto }) {
	const { t } = useTranslation(["tools", "common"]);
	const log = useLanguageServerLog(machineId, server.id);
	const lines = log.data?.lines ?? [];
	return (
		<DialogContent size="xl">
			<DialogHeader>
				<DialogTitle>{t("languageServers.logTitle", { server: server.name })}</DialogTitle>
				<DialogDescription className="truncate font-mono text-xs">{server.root}</DialogDescription>
			</DialogHeader>
			<DialogBody>
				{log.isError && (
					<p role="alert" className="text-destructive text-sm">
						{log.error.message}
					</p>
				)}
				<pre className="border-border bg-muted/40 min-h-0 overflow-auto rounded-md border p-2 font-mono text-[0.6875rem] leading-snug whitespace-pre-wrap">
					{log.isPending
						? t("common:state.loading")
						: lines.length > 0
							? lines.join("\n")
							: t("languageServers.logEmpty")}
				</pre>
			</DialogBody>
		</DialogContent>
	);
}
