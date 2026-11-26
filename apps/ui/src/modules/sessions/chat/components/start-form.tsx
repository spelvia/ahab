import { Bot, Brain, Sparkles } from "lucide-react";
import { useState } from "react";

import { AgentIcon, useAgents } from "@/modules/machines";
import { DropZone, useAttachments } from "@/modules/sessions/attachment";
import {
	readProjectCommands,
	useAgentDefaults,
	useModels,
	useRememberChoice,
} from "@/modules/sessions/chat/api";
import { Composer } from "@/modules/sessions/chat/components/composer";
import { FastToggle } from "@/modules/sessions/chat/components/fast-toggle";
import { ModePicker } from "@/modules/sessions/chat/components/mode-picker";
import { defaultMode, effortLabel, permissionModes } from "@/modules/sessions/chat/model";
import { draftStore } from "@/modules/sessions/reference";
import type {
	AgentModel,
	CommandInvocation,
	ContextReference as WireReference,
} from "@/shared/api/generated/ahab.schemas";
import { toWire } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { lastAgent, rememberAgent } from "@/shared/preferences/store";
import { Picker } from "@/shared/ui/picker";

/** What starting a chat needs. */
export interface ChatStart {
	agent: string;
	message: string;
	command?: CommandInvocation;
	model?: string;
	effort?: string;
	/** The permission mode; else the agent's default. */
	mode?: string;
	/** Fast mode, where the model offers it; else the provider's default. */
	fast?: boolean;
	attachments?: string[];
	/** Context for the agent. */
	references?: WireReference[];
}

/** The model to start with: the one last chosen, else the agent's default. */
function startModel(models: AgentModel[], remembered?: string): AgentModel | undefined {
	return (
		models.find((model) => model.id === remembered) ??
		models.find((model) => model.isDefault) ??
		models[0]
	);
}

/** The effort to start with: the one last chosen if the model has it, else its default. */
function startEffort(model: AgentModel | undefined, remembered?: string): string | undefined {
	if (!model || model.efforts.length === 0) return undefined;
	if (remembered && model.efforts.some((effort) => effort.id === remembered)) return remembered;
	return model.defaultEffort ?? undefined;
}

/**
 * Starting a chat: the agent (the ready ones on the machine), its model and
 * reasoning effort (the last choices first), and the first message. Enter
 * starts it.
 */
export function StartForm({
	projectId,
	machineId,
	machineName,
	offline,
	placeholder,
	pending,
	error,
	autoFocus = true,
	draftKey,
	onOpenFile,
	onStart,
}: {
	/** Where attached files are uploaded to. */
	projectId: string;
	machineId: string;
	machineName: string;
	offline?: boolean;
	placeholder?: string;
	pending?: boolean;
	error?: string;
	autoFocus?: boolean;
	/** The New session window's context draft (New session with this). */
	draftKey?: string;
	onOpenFile?: (path: string, line?: number) => void;
	onStart: (start: ChatStart) => void | Promise<unknown>;
}) {
	const { t } = useTranslation("sessions");
	const agents = useAgents(machineId);
	const attachments = useAttachments(offline ? undefined : projectId, draftKey);
	const ready = (agents.data?.items ?? []).filter((agent) => agent.state === "ready");
	const [chosenAgent, setAgent] = useState<string>();
	const agent =
		chosenAgent ?? ready.find((item) => item.agent === lastAgent())?.agent ?? ready[0]?.agent;
	const models = useModels(machineId, agent);
	const list = models.data?.items ?? [];
	// The agent's defaults: the last choices made for it, on any device.
	const defaults = useAgentDefaults();
	const remember = useRememberChoice();
	const remembered = defaults.data?.items.find((item) => item.agent === agent);
	const model = startModel(list, remembered?.model ?? undefined);
	const effort = startEffort(model, remembered?.effort ?? undefined);
	const mode = permissionModes(agent).some((item) => item.id === remembered?.mode)
		? (remembered?.mode ?? undefined)
		: defaultMode(agent);

	const [fast, setFast] = useState<boolean>();

	/** Every choice becomes the agent's default at once. */
	const choose = (change: { model?: string; effort?: string; mode?: string }) => {
		if (!agent) return;
		remember(agent, { model: model?.id, effort, mode, ...change });
	};

	return (
		<DropZone
			attachments={attachments}
			onReferences={draftKey ? (references) => draftStore.add(draftKey, references) : undefined}
		>
			<Composer
				autoFocus={autoFocus}
				commandChat={agent && !offline ? JSON.stringify([projectId, agent]) : undefined}
				projectId={projectId}
				commandPlacement="below"
				readCommands={(_, signal) =>
					agent ? readProjectCommands(projectId, agent, signal) : Promise.resolve([])
				}
				attachments={attachments}
				placeholder={
					offline
						? t("startForm.offline", { machine: machineName })
						: ready.length === 0
							? agents.isPending
								? t("startForm.checkingAgents")
								: t("startForm.noAgentReady", { machine: machineName })
							: (placeholder ?? t("startForm.placeholder"))
				}
				disabled={offline || !agent}
				pending={pending}
				draftKey={draftKey ?? `first:${projectId}`}
				onOpenFile={onOpenFile}
				onSend={async (message, attached, references, _delivery, command) => {
					if (!agent) return;
					rememberAgent(agent);
					await onStart({
						agent,
						message,
						...(command && { command }),
						model: model?.id,
						effort,
						mode,
						...(fast !== undefined && model?.fast ? { fast } : {}),
						attachments: attached,
						...(references.length ? { references: references.map(toWire) } : {}),
					});
					if (!draftKey) draftStore.clear(`first:${projectId}`);
				}}
				settingsSummary={[
					ready.find((item) => item.agent === agent)?.label,
					model?.label,
					effort ? effortLabel(effort) : undefined,
				]
					.filter(Boolean)
					.join(" · ")}
				settings={
					<>
						<Picker
							label={t("startForm.agent")}
							icon={<Bot />}
							value={agent}
							options={ready.map((item) => ({
								value: item.agent,
								label: item.label,
								icon: <AgentIcon agent={item.agent} />,
								note: item.version ?? undefined,
								description:
									item.agent === "pi"
										? t("startForm.agentDescription.pi")
										: item.agent === "codex"
											? t("startForm.agentDescription.codex")
											: t("startForm.agentDescription.other"),
							}))}
							onChange={setAgent}
						/>
						<Picker
							label={t("startForm.model")}
							icon={<Sparkles />}
							value={model?.id}
							disabled={models.isPending || list.length === 0}
							options={list.map((item) => ({
								value: item.id,
								label: item.label,
								note: item.isDefault ? t("startForm.defaultNote") : undefined,
								description: item.description || undefined,
							}))}
							onChange={(id) => {
								const next = list.find((item) => item.id === id);
								choose({ model: id, effort: startEffort(next, effort) });
							}}
						/>
						{model && model.efforts.length > 0 ? (
							<Picker
								label={t("startForm.reasoning")}
								icon={<Brain />}
								value={effort ?? ""}
								options={[
									// Without a default of its own, the agent decides.
									...(model.defaultEffort
										? []
										: [
												{
													value: "",
													label: t("startForm.defaultEffort"),
													description: t("startForm.defaultEffortDescription"),
												},
											]),
									...model.efforts.map((item) => ({
										value: item.id,
										label: effortLabel(item.id),
										note: item.id === model.defaultEffort ? t("startForm.defaultNote") : undefined,
										description: item.description || undefined,
									})),
								]}
								onChange={(id) => choose({ effort: id || undefined })}
							/>
						) : null}
						<ModePicker agent={agent} value={mode} onChange={(id) => choose({ mode: id })} />
						<FastToggle fast={model?.fast} value={fast} onChange={setFast} />
					</>
				}
			/>
			{models.isError ? (
				<p className="text-muted-foreground mt-2 text-sm">{t("startForm.modelsUnlisted")}</p>
			) : null}
			{pending ? (
				<p className="text-muted-foreground mt-2 text-sm">{t("startForm.starting")}</p>
			) : null}
			{error ? (
				<p role="alert" className="text-destructive mt-2 text-sm">
					{error}
				</p>
			) : null}
		</DropZone>
	);
}
