import { ExternalLink, ShieldQuestion } from "lucide-react";
import { useState } from "react";

import { useShownPath } from "@/modules/sessions/chat/components/chat-root";
import { Diff } from "@/modules/sessions/chat/components/diff-block";
import {
	Actions,
	AnswerError,
	fieldClass,
	Outcome,
	RequestFrame,
	type RequestProps,
	touch,
	useAnswer,
} from "@/modules/sessions/chat/components/request-frame";
import {
	type AgentChoice,
	choiceOf,
	outcomeSummary,
	type RequestPreview,
	requestPreview,
	requestSummary,
	ruleKeptIn,
	sessionScope,
} from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { HighlightedCode } from "@/shared/syntax/code";
import { Button } from "@/shared/ui/button";

/**
 * Permission to use a tool, shaped by the tool: a command and its folder,
 * the change as a diff, the address, else its input as fields. Allow once,
 * allow for the session (saying what that covers), deny with an optional
 * reason, or deny and stop. An ACP agent's own choices are its buttons.
 */
export function PermissionRequest({ chatId, request, pending, outcome, call }: RequestProps) {
	const { t } = useTranslation("sessions");
	const shown = useShownPath();
	const answer = useAnswer(chatId, request.id);
	const [reasonOpen, setReasonOpen] = useState(false);
	const [reason, setReason] = useState("");
	const options: AgentChoice[] = request.kind.type === "permission" ? request.kind.options : [];
	const canForSession = request.kind.type === "tool" && request.kind.canAllowForSession;
	const scope = canForSession ? sessionScope(request) : undefined;
	const kept = ruleKeptIn(request);
	const preview = requestPreview(request, call);
	const select = (choice: AgentChoice) => answer.send({ decision: "select", optionId: choice.id });
	const allow = () => {
		const once = choiceOf(options, ["allowOnce", "allowAlways"]);
		if (once) select(once);
		else answer.send({ decision: "allow" });
	};
	const deny = () => {
		const refuse = choiceOf(options, ["rejectOnce", "rejectAlways"]);
		if (refuse) select(refuse);
		else answer.send({ decision: "deny", message: reason.trim() || undefined });
	};
	const stop = () => answer.send({ decision: "cancel" });
	return (
		<RequestFrame
			request={request}
			pending={pending}
			icon={<ShieldQuestion />}
			title={requestSummary(request, shown)}
			onAllow={allow}
			onDeny={deny}
			shortcuts={t("permission.shortcuts")}
		>
			{pending ? <Preview preview={preview} /> : null}
			{pending ? (
				<>
					<Actions>
						{options.length > 0 ? (
							options.map((choice, index) => (
								<Button
									key={choice.id}
									className={touch}
									variant={
										choice.kind === "allowOnce" && index === firstAllow(options)
											? "default"
											: choice.kind === "allowOnce" || choice.kind === "allowAlways"
												? "outline"
												: "ghost"
									}
									title={choice.description ?? undefined}
									disabled={answer.pending}
									onClick={() => select(choice)}
								>
									{choice.label}
								</Button>
							))
						) : (
							<>
								<Button className={touch} disabled={answer.pending} onClick={allow}>
									{t("permission.allow")}
								</Button>
								{canForSession ? (
									<Button
										className={touch}
										variant="outline"
										disabled={answer.pending}
										onClick={() => answer.send({ decision: "allowForSession" })}
									>
										{t("permission.allowForSession")}
									</Button>
								) : null}
								<Button className={touch} variant="ghost" disabled={answer.pending} onClick={deny}>
									{t("permission.deny")}
								</Button>
							</>
						)}
						<Button
							className={`${touch} text-destructive hover:text-destructive`}
							variant="ghost"
							disabled={answer.pending}
							onClick={stop}
						>
							{t("permission.denyAndStop")}
						</Button>
						{options.length === 0 && !reasonOpen ? (
							<Button
								className={`${touch} text-muted-foreground`}
								variant="ghost"
								disabled={answer.pending}
								onClick={() => setReasonOpen(true)}
							>
								{t("permission.addReason")}
							</Button>
						) : null}
					</Actions>
					{scope ? (
						<p className="text-muted-foreground text-xs text-pretty">
							{kept
								? t(`permission.sessionCoversKept.${kept}`, { scope })
								: t("permission.sessionCovers", { scope })}
						</p>
					) : null}
					{reasonOpen ? (
						<label className="flex flex-col gap-1.5">
							<span className="text-muted-foreground text-xs font-medium">
								{t("permission.reasonLabel")}
							</span>
							<input
								// The field was asked for: it takes the focus.
								// oxlint-disable-next-line jsx-a11y/no-autofocus
								autoFocus
								value={reason}
								placeholder={t("permission.reasonPlaceholder")}
								onChange={(event) => setReason(event.target.value)}
								onKeyDown={(event) => {
									if (event.key === "Enter" && !event.nativeEvent.isComposing) {
										event.preventDefault();
										deny();
									}
								}}
								className={fieldClass}
							/>
						</label>
					) : null}
					<AnswerError error={answer.error} />
				</>
			) : (
				<Outcome>{outcomeSummary(request, outcome)}</Outcome>
			)}
		</RequestFrame>
	);
}

function firstAllow(options: readonly AgentChoice[]): number {
	return options.findIndex((choice) => choice.kind === "allowOnce");
}

/** What the tool would do, as the tool shows best. */
function Preview({ preview }: { preview: RequestPreview }) {
	const { t } = useTranslation("sessions");
	const shown = useShownPath();
	switch (preview.type) {
		case "command":
			return (
				<div className="flex flex-col gap-1">
					<pre className="bg-background/70 border-border max-h-60 overflow-auto rounded-md border px-3 py-2 font-mono text-xs leading-5 whitespace-pre-wrap">
						<HighlightedCode text={preview.command} as={{ language: "bash" }} />
					</pre>
					{preview.cwd ? (
						<p className="text-muted-foreground truncate font-mono text-xs" title={preview.cwd}>
							{t("permission.folder", { folder: shown(preview.cwd) })}
						</p>
					) : null}
				</div>
			);
		case "diff":
			return (
				<div className="flex flex-col gap-2">
					{preview.files.map((file) => (
						<div key={file.path} className="min-w-0">
							<p
								className="text-muted-foreground mb-1 truncate font-mono text-xs"
								title={file.path}
							>
								{shown(file.path)}
							</p>
							<Diff text={file.diff} path={file.path} />
						</div>
					))}
				</div>
			);
		case "url":
			return (
				<div className="flex min-w-0 flex-col gap-1">
					<a
						href={preview.url}
						target="_blank"
						rel="noreferrer noopener"
						className="text-heading flex min-w-0 items-center gap-1.5 font-mono text-xs underline-offset-2 hover:underline"
					>
						<span className="truncate">{preview.url}</span>
						<ExternalLink className="size-3.5 shrink-0" aria-hidden />
					</a>
					{preview.prompt ? (
						<p className="text-muted-foreground text-xs text-pretty">
							{t("permission.prompt", { prompt: preview.prompt })}
						</p>
					) : null}
				</div>
			);
		case "search":
			return (
				<p className="text-sm">
					<span className="text-muted-foreground">{t("permission.search")} </span>
					<span className="font-mono text-xs">{preview.query}</span>
				</p>
			);
		case "fields":
			return <Fields fields={preview.fields} label={t("permission.input")} />;
		default:
			return null;
	}
}

/** A tool's input as names and values: short ones in a line, long ones in a box. */
export function Fields({
	fields,
	label,
}: {
	fields: { key: string; value: string; long: boolean }[];
	label: string;
}) {
	return (
		<dl aria-label={label} className="border-border border-t text-xs">
			{fields.map((field) => (
				<div
					key={field.key}
					className="border-border grid gap-x-4 gap-y-1 border-b py-1.5 sm:grid-cols-[8rem_minmax(0,1fr)]"
				>
					<dt className="text-muted-foreground truncate font-mono" title={field.key}>
						{field.key}
					</dt>
					<dd className="min-w-0">
						{field.long ? (
							<pre className="bg-background/70 max-h-48 overflow-auto rounded-md p-2 font-mono leading-5 whitespace-pre-wrap">
								<HighlightedCode
									text={field.value}
									as={/^[{[]/.test(field.value.trim()) ? { language: "json" } : undefined}
								/>
							</pre>
						) : (
							<span className="font-mono break-words">
								<HighlightedCode
									text={field.value}
									as={/^[{[]/.test(field.value.trim()) ? { language: "json" } : undefined}
								/>
							</span>
						)}
					</dd>
				</div>
			))}
		</dl>
	);
}
