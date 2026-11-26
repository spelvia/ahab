import { ArrowUpCircle } from "lucide-react";
import { useState } from "react";

import { useUpdateAgent } from "@/modules/machines/agents/api";
import type { AgentUpdate, Machine } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { toast } from "@/shared/ui/toast";

/** The installers known by name; `native` and `unknown` are said in words. */
const METHOD: Record<string, string> = {
	npm: "npm",
	homebrew: "Homebrew",
	nix: "Nix",
};

/**
 * An agent's version against the newest release: up to date, or a newer
 * one out with a button that installs it (the agent's own updater, after a
 * confirmation), or how to update it when Ahab cannot (Nix).
 */
export function AgentUpdateLine({ machine, update }: { machine: Machine; update: AgentUpdate }) {
	const { t } = useTranslation("machines");
	const [asking, setAsking] = useState(false);
	const run = useUpdateAgent(machine.id);
	const result = run.data;
	const method =
		update.method === "native" || update.method === "unknown"
			? t(`agentUpdate.method.${update.method}`)
			: (METHOD[update.method] ?? update.method);

	return (
		<div className="space-y-1.5">
			<p className="flex flex-wrap items-center gap-x-2 gap-y-1">
				{update.available ? (
					<>
						<ArrowUpCircle className="text-primary size-4" />
						<span>
							<Trans
								t={t}
								i18nKey="agentUpdate.available"
								values={{ latest: update.latest ?? "", installed: update.installed ?? "?" }}
								components={{ version: <span className="font-mono text-xs" /> }}
							/>
						</span>
					</>
				) : update.latest ? (
					<span className="text-muted-foreground">
						<Trans
							t={t}
							i18nKey="agentUpdate.upToDate"
							values={{ latest: update.latest ?? "", channel: update.channel }}
							components={{ version: <span className="font-mono text-xs" /> }}
						/>
					</span>
				) : (
					<span className="text-muted-foreground">
						{update.reason
							? t("agentUpdate.unknownBecause", { reason: update.reason })
							: t("agentUpdate.unknown")}
					</span>
				)}
				<span className="text-muted-foreground text-xs">
					· {t("agentUpdate.installedWith", { method })}
				</span>
				{update.available && update.canUpdate && (
					<Button
						size="xs"
						variant="outline"
						className="ml-auto"
						disabled={!machine.online || run.isPending}
						onClick={() => setAsking(true)}
					>
						{run.isPending
							? t("agentUpdate.updating")
							: t("agentUpdate.updateTo", { version: update.latest ?? "" })}
					</Button>
				)}
			</p>
			{update.available && !update.canUpdate && (
				<p className="text-muted-foreground text-xs">
					{t("agentUpdate.elsewhere", { reason: update.reason ?? "", command: update.command })}
				</p>
			)}
			{run.isPending && (
				<p className="text-muted-foreground text-xs" role="status">
					<Trans
						t={t}
						i18nKey="agentUpdate.running"
						values={{ command: update.command, machine: machine.name }}
						components={{ code: <code className="font-mono" /> }}
					/>
				</p>
			)}
			{run.isError && (
				<p role="alert" className="text-destructive text-xs">
					{t("agentUpdate.failed", { message: run.error.message })}
				</p>
			)}
			{result && (
				<div className="text-xs" role="status">
					<p className={result.success ? "text-success" : "text-destructive"}>
						{result.success
							? result.after && result.after !== result.before
								? t("agentUpdate.updated", {
										before: result.before ?? "?",
										after: result.after ?? "",
									})
								: t("agentUpdate.unchanged", {
										version: result.after ?? t("agentUpdate.unknownVersion"),
									})
							: t("agentUpdate.updaterError")}
					</p>
					{result.output && (
						<details className="mt-1">
							<summary className="text-muted-foreground cursor-pointer">
								{t("agentUpdate.output")}
							</summary>
							<pre className="bg-muted mt-1 max-h-48 overflow-auto rounded-md p-2 font-mono text-[0.6875rem] whitespace-pre-wrap">
								{result.output}
							</pre>
						</details>
					)}
				</div>
			)}
			<ConfirmDialog
				open={asking}
				onOpenChange={setAsking}
				title={t("agentUpdate.confirmTitle", { agent: update.label, machine: machine.name })}
				description={t("agentUpdate.confirmDescription", {
					command: update.command,
					version: update.latest ?? "",
					agent: update.label,
					machine: machine.name,
				})}
				action={t("agentUpdate.update")}
				pending={run.isPending}
				onConfirm={() => {
					setAsking(false);
					run.mutate({ id: machine.id, agent: update.agent });
				}}
			/>
		</div>
	);
}

/**
 * A list row's Update: the same confirmation, then the outcome as a toast
 * (the agent's drawer shows what the updater printed).
 */
export function UpdateButton({ machine, update }: { machine: Machine; update: AgentUpdate }) {
	const { t } = useTranslation("machines");
	const [asking, setAsking] = useState(false);
	const run = useUpdateAgent(machine.id);
	return (
		<>
			<Button
				size="xs"
				variant="outline"
				disabled={!machine.online || run.isPending}
				onClick={(event) => {
					event.stopPropagation();
					setAsking(true);
				}}
			>
				{run.isPending ? t("agentUpdate.updating") : t("agentUpdate.update")}
			</Button>
			<ConfirmDialog
				open={asking}
				onOpenChange={setAsking}
				title={t("agentUpdate.confirmTitle", { agent: update.label, machine: machine.name })}
				description={t("agentUpdate.confirmDescription", {
					command: update.command,
					version: update.latest ?? "",
					agent: update.label,
					machine: machine.name,
				})}
				action={t("agentUpdate.update")}
				pending={run.isPending}
				onConfirm={() => {
					setAsking(false);
					run.mutate(
						{ id: machine.id, agent: update.agent },
						{
							onSuccess: (result) =>
								result.success
									? toast(
											result.after
												? t("agentUpdate.toastUpdated", {
														agent: update.label,
														version: result.after,
														machine: machine.name,
													})
												: t("agentUpdate.toastUpdatedNoVersion", {
														agent: update.label,
														machine: machine.name,
													}),
										)
									: toast(
											t("agentUpdate.toastUpdaterError", {
												agent: update.label,
												machine: machine.name,
											}),
											{ duration: 8000 },
										),
							onError: (error) =>
								toast(
									t("agentUpdate.toastFailed", { agent: update.label, message: error.message }),
									{ duration: 8000 },
								),
						},
					);
				}}
			/>
		</>
	);
}
