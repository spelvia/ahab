import { cn } from "cn";
import { ClipboardCheck } from "lucide-react";
import { useRef, useState } from "react";

import { useShownPath } from "@/modules/sessions/chat/components/chat-root";
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
import { outcomeSummary } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";

/**
 * The agent's plan, as Markdown, waiting for approval: its own ways to
 * approve it (Claude Code: accept edits, or ask before each), or "Keep
 * planning" with what should change.
 */
export function PlanRequest({ chatId, request, pending, outcome, agent }: RequestProps) {
	const { t } = useTranslation("sessions");
	const shown = useShownPath();
	const answer = useAnswer(chatId, request.id);
	const [feedbackOpen, setFeedbackOpen] = useState(false);
	const [feedback, setFeedback] = useState("");
	const [planOpen, setPlanOpen] = useState(false);
	const [whole, setWhole] = useState(false);
	const field = useRef<HTMLTextAreaElement>(null);
	if (request.kind.type !== "planApproval") return null;
	const { plan, options, path } = request.kind;
	const approve = () => {
		const first = options[0];
		answer.send(first ? { decision: "select", optionId: first.id } : { decision: "allow" });
	};
	const keepPlanning = () => {
		if (!feedbackOpen) {
			setFeedbackOpen(true);
			return;
		}
		answer.send({ decision: "deny", message: feedback.trim() || undefined });
	};
	const planShown = pending || planOpen;
	return (
		<RequestFrame
			request={request}
			pending={pending}
			icon={<ClipboardCheck />}
			title={t("plan.title", { agent })}
			onAllow={approve}
			onDeny={keepPlanning}
			shortcuts={t("plan.shortcuts")}
		>
			{path ? (
				<p className="text-muted-foreground -mt-1 truncate font-mono text-xs" title={path}>
					{t("plan.savedTo", { path: shown(path) })}
				</p>
			) : null}
			{planShown ? (
				<div className="relative">
					<div
						className={cn(
							"border-border bg-background/70 overflow-hidden rounded-md border px-3.5 py-3",
							!whole && "max-h-[28rem] overflow-y-auto",
						)}
					>
						<Markdown text={plan} />
					</div>
					{!whole && plan.split("\n").length > 24 ? (
						<Button
							variant="ghost"
							size="sm"
							className="text-muted-foreground mt-1"
							onClick={() => setWhole(true)}
						>
							{t("plan.showAll")}
						</Button>
					) : null}
				</div>
			) : null}
			{pending ? (
				<>
					<Actions>
						{options.length > 0 ? (
							options.map((option, index) => (
								<Button
									key={option.id}
									className={touch}
									variant={index === 0 ? "default" : "outline"}
									title={option.description ?? undefined}
									disabled={answer.pending}
									onClick={() => answer.send({ decision: "select", optionId: option.id })}
								>
									{option.label}
								</Button>
							))
						) : (
							<Button className={touch} disabled={answer.pending} onClick={approve}>
								{t("plan.approve")}
							</Button>
						)}
						{feedbackOpen ? null : (
							<Button
								className={touch}
								variant="ghost"
								disabled={answer.pending}
								onClick={() => {
									setFeedbackOpen(true);
									requestAnimationFrame(() => field.current?.focus());
								}}
							>
								{t("plan.keepPlanning")}
							</Button>
						)}
					</Actions>
					{feedbackOpen ? (
						<div className="flex flex-col gap-2">
							<label className="flex flex-col gap-1.5">
								<span className="text-muted-foreground text-xs font-medium">
									{t("plan.feedbackLabel")}
								</span>
								<textarea
									ref={field}
									// The field was asked for: it takes the focus.
									// oxlint-disable-next-line jsx-a11y/no-autofocus
									autoFocus
									rows={3}
									value={feedback}
									placeholder={t("plan.feedbackPlaceholder")}
									onChange={(event) => setFeedback(event.target.value)}
									onKeyDown={(event) => {
										if (
											event.key === "Enter" &&
											(event.metaKey || event.ctrlKey) &&
											!event.nativeEvent.isComposing
										) {
											event.preventDefault();
											keepPlanning();
										}
									}}
									className={cn(fieldClass, "resize-y")}
								/>
							</label>
							<Actions>
								<Button
									className={touch}
									variant="outline"
									disabled={answer.pending}
									onClick={keepPlanning}
								>
									{t("plan.sendFeedback")}
								</Button>
								<Button
									className={touch}
									variant="ghost"
									disabled={answer.pending}
									onClick={() => setFeedbackOpen(false)}
								>
									{t("plan.cancel")}
								</Button>
							</Actions>
						</div>
					) : null}
					<AnswerError error={answer.error} />
				</>
			) : (
				<div className="flex flex-wrap items-center gap-x-3 gap-y-1">
					<Outcome>{outcomeSummary(request, outcome)}</Outcome>
					<Button
						variant="ghost"
						size="sm"
						className="text-muted-foreground -ml-2 sm:ml-0"
						aria-expanded={planOpen}
						onClick={() => setPlanOpen(!planOpen)}
					>
						{planOpen ? t("plan.hidePlan") : t("plan.showPlan")}
					</Button>
				</div>
			)}
		</RequestFrame>
	);
}
