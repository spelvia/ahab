import { cn } from "cn";
import { Bot, Folder as FolderIcon, SquareTerminal } from "lucide-react";
import { useState } from "react";

import { StartForm } from "@/modules/sessions/chat";
import { draftStore } from "@/modules/sessions/reference";
import { useCreateSession } from "@/modules/sessions/session/api";
import { useStartTerminal } from "@/modules/sessions/terminal";
import type { Session, Project } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/** What a new session runs: an agent, or a terminal. */
export type NewKind = "agent" | "terminal";

/**
 * Start a session in the current project: an agent (its first chat's
 * agent, model and message), or a terminal (the login shell, or a command).
 */
export function NewSession({
	project,
	folder,
	initialKind = "agent",
	draftKey,
	onOpenFile,
	onStarted,
}: {
	project: Project;
	/** The folder it is filed in once started. */
	folder?: { id: string; name: string };
	initialKind?: NewKind;
	/** The window's context draft (New session with this). */
	draftKey?: string;
	onOpenFile?: (path: string, line?: number) => void;
	onStarted: (session: Session) => void;
}) {
	const { t } = useTranslation("sessions");
	const [kind, setKind] = useState<NewKind>(initialKind);
	const create = useCreateSession();
	const offline = project.availability !== "available";
	return (
		<div className="w-full max-w-2xl">
			<div className="mb-4 flex items-center justify-between gap-3">
				<div className="min-w-0">
					<h2 className="font-heading text-heading text-lg font-semibold">
						{t("newSession.title")}
					</h2>
					{folder ? (
						<p className="text-muted-foreground flex items-center gap-1 text-sm">
							<FolderIcon className="size-3.5 shrink-0" aria-hidden />
							<span className="truncate">{t("newSession.inFolder", { folder: folder.name })}</span>
						</p>
					) : null}
				</div>
				<div
					role="radiogroup"
					aria-label={t("newSession.kind")}
					className="bg-muted flex rounded-md p-0.5 text-sm"
				>
					{(
						[
							["agent", t("newSession.agent"), <Bot key="i" className="size-4" />],
							["terminal", t("newSession.terminal"), <SquareTerminal key="i" className="size-4" />],
						] as const
					).map(([value, label, icon]) => (
						<button
							key={value}
							type="button"
							role="radio"
							aria-checked={kind === value}
							onClick={() => setKind(value)}
							className={cn(
								"focus-visible:ring-ring flex items-center gap-1.5 rounded px-2.5 py-1 outline-none focus-visible:ring-2",
								kind === value
									? "bg-background text-foreground shadow-sm"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							{icon}
							{label}
						</button>
					))}
				</div>
			</div>
			{kind === "agent" ? (
				<StartForm
					projectId={project.id}
					machineId={project.machineId}
					machineName={project.machineName}
					offline={offline}
					pending={create.isPending}
					error={create.error?.message}
					draftKey={draftKey}
					onOpenFile={onOpenFile}
					onStart={(chat) =>
						create.mutateAsync(
							{ project: project.id, data: { chat, folderId: folder?.id ?? null } },
							{
								onSuccess: (started) => {
									if (draftKey) draftStore.clear(draftKey);
									onStarted(started);
								},
							},
						)
					}
				/>
			) : (
				<TerminalForm
					project={project}
					folderId={folder?.id}
					offline={offline}
					onStarted={onStarted}
				/>
			)}
		</div>
	);
}

/** A terminal in the project's folder: the login shell, or one command. */
function TerminalForm({
	project,
	folderId,
	offline,
	onStarted,
}: {
	project: Project;
	folderId?: string;
	offline: boolean;
	onStarted: (session: Session) => void;
}) {
	const { t } = useTranslation("sessions");
	const start = useStartTerminal();
	const [command, setCommand] = useState("");
	return (
		<form
			className="border-border bg-card flex flex-col gap-3 rounded-lg border p-4"
			onSubmit={(event) => {
				event.preventDefault();
				const run = command.trim();
				start.mutate(
					{
						id: project.id,
						data: { cols: 100, rows: 30, command: run || null, folderId: folderId ?? null },
					},
					{ onSuccess: onStarted },
				);
			}}
		>
			<p className="text-muted-foreground text-sm">
				{offline
					? t("terminalForm.offline", { machine: project.machineName })
					: t("terminalForm.description", {
							path: project.path,
							machine: project.machineName,
						})}
			</p>
			<label className="flex flex-col gap-1.5 text-sm">
				<span>
					<Trans
						t={t}
						i18nKey="terminalForm.command"
						components={{ muted: <span className="text-muted-foreground" /> }}
					/>
				</span>
				<input
					value={command}
					onChange={(event) => setCommand(event.target.value)}
					placeholder={t("terminalForm.commandPlaceholder")}
					autoFocus
					spellCheck={false}
					className="border-input bg-background focus-visible:ring-ring rounded-md border px-2.5 py-1.5 font-mono text-sm outline-none focus-visible:ring-2"
				/>
			</label>
			{start.error ? <p className="text-destructive text-sm">{start.error.message}</p> : null}
			<div className="flex justify-end">
				<Button type="submit" disabled={offline || start.isPending}>
					<SquareTerminal />
					{start.isPending ? t("terminalForm.starting") : t("terminalForm.start")}
				</Button>
			</div>
		</form>
	);
}
