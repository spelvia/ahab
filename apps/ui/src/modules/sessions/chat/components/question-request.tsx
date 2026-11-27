import { cn } from "cn";
import { Check, MessageCircleQuestion } from "lucide-react";
import { useId, useRef, useState } from "react";

import {
	AnswerError,
	fieldClass,
	Outcome,
	RequestFrame,
	type RequestProps,
	touch,
	useAnswer,
} from "@/modules/sessions/chat/components/request-frame";
import {
	type AgentQuestion,
	answered,
	answerFrom,
	type Draft,
	emptyDraft,
	outcomeSummary,
	shownAnswers,
	toggleOption,
	toggleOther,
} from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";

/**
 * The agent's questions, one step at a time: each with its options to pick
 * (one or several), "Other…" for your own words where it takes them, and a
 * hidden field for a secret. Answered, it shows what you chose.
 */
export function QuestionRequest({ chatId, request, pending, outcome, agent }: RequestProps) {
	const { t } = useTranslation("sessions");
	const answer = useAnswer(chatId, request.id);
	const questions = request.kind.type === "question" ? request.kind.questions : [];
	const [step, setStep] = useState(0);
	const [drafts, setDrafts] = useState<Record<string, Draft>>({});
	const card = useRef<HTMLDivElement>(null);
	const question = questions[Math.min(step, questions.length - 1)];
	const last = step >= questions.length - 1;
	const done = questions.every((each) => answered(each, drafts[each.id]));
	const ready = question ? answered(question, drafts[question.id]) : false;
	const submit = () => {
		if (done && !answer.pending) answer.send(answerFrom(questions, drafts));
	};
	const forward = () => {
		if (!ready || answer.pending) return;
		if (last) submit();
		else {
			setStep(step + 1);
			// The next question's first choice, for the keyboard.
			requestAnimationFrame(() =>
				card.current?.querySelector<HTMLElement>("[data-choice], input")?.focus(),
			);
		}
	};
	const decline = () => answer.send({ decision: "deny" });
	if (!question) return null;
	const total = questions.length;
	return (
		<RequestFrame
			request={request}
			pending={pending}
			questionLayout={pending}
			icon={<MessageCircleQuestion />}
			title={pending ? question.question : t("question.title", { agent })}
			meta={
				pending && (question.header || total > 1) ? (
					<span className="flex flex-wrap items-center gap-x-3">
						{question.header && <span>{question.header}</span>}
						{total > 1 && <span>{t("question.step", { current: step + 1, total })}</span>}
					</span>
				) : undefined
			}
			onAllow={forward}
			onDeny={answer.pending ? undefined : decline}
		>
			{pending ? (
				<div ref={card} className="flex flex-col gap-3">
					<fieldset disabled={answer.pending} className="min-w-0">
						<QuestionStep
							key={question.id}
							question={question}
							draft={drafts[question.id] ?? emptyDraft}
							onChange={(draft) => setDrafts({ ...drafts, [question.id]: draft })}
							onEnter={forward}
						/>
					</fieldset>
					<div className="border-border flex flex-wrap items-center gap-1 border-t pt-2">
						<Button
							className={cn(touch, "mr-auto text-muted-foreground")}
							variant="ghost"
							disabled={answer.pending}
							onClick={decline}
						>
							{t("question.decline")}
						</Button>
						{step > 0 ? (
							<Button
								className={touch}
								variant="ghost"
								disabled={answer.pending}
								onClick={() => setStep(step - 1)}
							>
								{t("question.back")}
							</Button>
						) : null}
						{last ? (
							<Button className={touch} disabled={!done || answer.pending} onClick={submit}>
								{t("question.submit")}
							</Button>
						) : (
							<Button className={touch} disabled={!ready} onClick={forward}>
								{t("question.next")}
							</Button>
						)}
					</div>
					<AnswerError error={answer.error} />
				</div>
			) : (
				<Answered request={request} outcome={outcome} />
			)}
		</RequestFrame>
	);
}

/** Each question with what was answered, or how the request ended. */
function Answered({ request, outcome }: Pick<RequestProps, "request" | "outcome">) {
	const given = shownAnswers(request, outcome);
	if (!given.length) return <Outcome>{outcomeSummary(request, outcome)}</Outcome>;
	return (
		<dl className="flex flex-col gap-2 text-sm">
			{given.map(({ question, answer }) => (
				<div key={question.id} className="min-w-0">
					<dt className="text-muted-foreground text-pretty">{question.question}</dt>
					<dd className="text-heading font-medium break-words">{answer}</dd>
				</div>
			))}
		</dl>
	);
}

/** One question: its header, options and own-words field. */
function QuestionStep({
	question,
	draft,
	onChange,
	onEnter,
}: {
	question: AgentQuestion;
	draft: Draft;
	onChange: (draft: Draft) => void;
	onEnter: () => void;
}) {
	const { t } = useTranslation("sessions");
	const groupName = useId();
	const [focused, setFocused] = useState<string>();
	const options = question.options;
	const freeOnly = options.length === 0;
	const showOther = freeOnly || (question.allowOther && draft.otherOn);
	// The option shown beside its preview: the one focused, else the one picked.
	const previewed = options.find(
		(option) => option.preview && option.label === (focused ?? draft.selected.at(-1)),
	);
	const role = question.multiSelect ? "checkbox" : "radio";
	return (
		<div className="flex flex-col gap-2.5">
			{options.length > 0 || question.allowOther ? (
				<div
					role={question.multiSelect ? "group" : "radiogroup"}
					aria-label={question.question}
					className="flex flex-col gap-0.5"
				>
					<span className={question.multiSelect ? "text-muted-foreground mb-1 text-xs" : "sr-only"}>
						{question.multiSelect ? t("question.pickMany") : t("question.pickOne")}
					</span>
					{options.map((option) => {
						const on = draft.selected.includes(option.label);
						return (
							<Choice
								key={option.label}
								role={role}
								name={groupName}
								on={on}
								label={option.label}
								description={option.description}
								onFocus={() => setFocused(option.label)}
								onBlur={() => setFocused(undefined)}
								onPick={() => onChange(toggleOption(question, draft, option.label))}
							/>
						);
					})}
					{question.allowOther && !freeOnly ? (
						<Choice
							role={role}
							name={groupName}
							on={draft.otherOn}
							label={t("question.other")}
							onPick={() => onChange(toggleOther(question, draft))}
						/>
					) : null}
				</div>
			) : null}
			{previewed?.preview ? (
				<div
					aria-label={t("question.preview", { option: previewed.label })}
					className="border-border bg-background/60 max-h-72 overflow-auto rounded-md border px-3 py-2 text-sm"
				>
					<Markdown text={previewed.preview} />
				</div>
			) : null}
			{showOther ? (
				<label className="flex flex-col gap-1.5">
					<span className={freeOnly ? "text-muted-foreground text-xs font-medium" : "sr-only"}>
						{question.secret ? t("question.secretLabel") : t("question.otherLabel")}
					</span>
					<input
						type={question.secret ? "password" : "text"}
						autoComplete={question.secret ? "off" : undefined}
						value={draft.other}
						placeholder={t("question.otherPlaceholder")}
						onChange={(event) => onChange({ ...draft, other: event.target.value })}
						onKeyDown={(event) => {
							if (event.key === "Enter" && !event.nativeEvent.isComposing) {
								event.preventDefault();
								onEnter();
							}
						}}
						className={fieldClass}
					/>
				</label>
			) : null}
		</div>
	);
}

/** A native choice keeps keyboard navigation and the full row's click target together. */
function Choice({
	role,
	name,
	on,
	label,
	description,
	onPick,
	onFocus,
	onBlur,
}: {
	role: "radio" | "checkbox";
	name: string;
	on: boolean;
	label: string;
	description?: string | null;
	onPick: () => void;
	onFocus?: () => void;
	onBlur?: () => void;
}) {
	const descriptionId = useId();
	return (
		<label
			className={cn(
				"relative flex min-h-11 cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring sm:min-h-10",
				on ? "bg-primary/8" : "hover:bg-accent/60",
			)}
		>
			<input
				type={role}
				name={name}
				checked={on}
				data-choice
				aria-label={label}
				aria-describedby={description ? descriptionId : undefined}
				onChange={onPick}
				onFocus={onFocus}
				onBlur={onBlur}
				className="absolute inset-0 z-10 m-0 h-full w-full cursor-pointer opacity-0"
			/>
			<span
				aria-hidden="true"
				className={cn(
					"grid size-4 shrink-0 place-items-center border border-muted-foreground/60",
					role === "radio" ? "rounded-full" : "rounded",
					on && "border-primary",
					on && role === "checkbox" && "bg-primary text-primary-foreground",
				)}
			>
				{on &&
					(role === "radio" ? (
						<span className="bg-primary size-2 rounded-full" />
					) : (
						<Check className="size-3" strokeWidth={3} />
					))}
			</span>
			<span className="min-w-0">
				<span className="block font-medium break-words">{label}</span>
				{description && (
					<span id={descriptionId} className="text-muted-foreground block text-xs break-words">
						{description}
					</span>
				)}
			</span>
		</label>
	);
}
