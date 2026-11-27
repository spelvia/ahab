import { cn } from "cn";
import { ExternalLink, FileInput } from "lucide-react";
import { useId, useState } from "react";

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
	type AgentFormField,
	fieldProblem,
	formDefaults,
	formValues,
	outcomeSummary,
} from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

/**
 * A form an MCP server or ACP agent asks to have filled in, from its
 * fields: text, numbers within their bounds, yes or no, one or several
 * choices. A form that is a page to open opens it, then says it is done.
 */
export function FormRequest({ chatId, request, pending, outcome, agent }: RequestProps) {
	const { t } = useTranslation("sessions");
	const answer = useAnswer(chatId, request.id);
	const fields = request.kind.type === "form" ? request.kind.fields : [];
	const [values, setValues] = useState<Record<string, unknown>>(() => formDefaults(fields));
	const [tried, setTried] = useState(false);
	if (request.kind.type !== "form") return null;
	const { message, source, url } = request.kind;
	const problems = Object.fromEntries(
		fields.map((field) => [field.name, fieldProblem(field, values[field.name])]),
	);
	const valid = Object.values(problems).every((problem) => !problem);
	const submit = () => {
		setTried(true);
		if (url) answer.send({ decision: "submit", values: {} });
		else if (valid) answer.send({ decision: "submit", values: formValues(fields, values) });
	};
	const decline = () => answer.send({ decision: "deny" });
	return (
		<RequestFrame
			request={request}
			pending={pending}
			icon={<FileInput />}
			title={source ? t("form.title", { source }) : t("form.titleUnknown", { agent })}
			onAllow={submit}
			onDeny={decline}
			shortcuts={t("form.shortcuts")}
		>
			{message ? <p className="text-foreground text-sm text-pretty">{message}</p> : null}
			{pending ? (
				<form
					noValidate
					className="flex flex-col gap-3"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					{url ? (
						<div className="flex flex-col gap-1.5">
							<a
								href={url}
								target="_blank"
								rel="noreferrer noopener"
								className="text-heading flex min-w-0 items-center gap-1.5 font-mono text-xs underline-offset-2 hover:underline"
							>
								<span className="truncate">{url}</span>
								<ExternalLink className="size-3.5 shrink-0" aria-hidden />
							</a>
							<p className="text-muted-foreground text-xs">{t("form.pageNote")}</p>
						</div>
					) : (
						fields.map((field) => (
							<FieldInput
								key={field.name}
								field={field}
								value={values[field.name]}
								problem={tried ? problems[field.name] : undefined}
								onChange={(value) => setValues({ ...values, [field.name]: value })}
							/>
						))
					)}
					<Actions>
						{url ? (
							<>
								<Button asChild className={touch}>
									<a href={url} target="_blank" rel="noreferrer noopener">
										{t("form.openPage")}
										<ExternalLink aria-hidden />
									</a>
								</Button>
								<Button type="submit" className={touch} variant="outline" disabled={answer.pending}>
									{t("form.pageDone")}
								</Button>
							</>
						) : (
							<Button type="submit" className={touch} disabled={answer.pending}>
								{t("form.submit")}
							</Button>
						)}
						<Button
							type="button"
							className={touch}
							variant="ghost"
							disabled={answer.pending}
							onClick={decline}
						>
							{t("form.decline")}
						</Button>
						<Button
							type="button"
							className={cn(touch, "text-destructive hover:text-destructive")}
							variant="ghost"
							disabled={answer.pending}
							onClick={() => answer.send({ decision: "cancel" })}
						>
							{t("form.cancel")}
						</Button>
					</Actions>
					<AnswerError error={answer.error} />
				</form>
			) : (
				<Outcome>{outcomeSummary(request, outcome)}</Outcome>
			)}
		</RequestFrame>
	);
}

/** One field, by its kind, with its title, description and problem. */
function FieldInput({
	field,
	value,
	problem,
	onChange,
}: {
	field: AgentFormField;
	value: unknown;
	problem?: string;
	onChange: (value: unknown) => void;
}) {
	const { t } = useTranslation("sessions");
	const id = useId();
	const label = field.title || field.name;
	const hint = field.description ? `${id}-hint` : undefined;
	const error = problem ? `${id}-problem` : undefined;
	const described = [hint, error].filter(Boolean).join(" ") || undefined;
	const common = {
		id,
		"aria-describedby": described,
		"aria-invalid": problem ? true : undefined,
		"aria-required": field.required || undefined,
	};
	let control;
	switch (field.kind) {
		case "boolean":
			control = (
				<Switch
					checked={value === true}
					onCheckedChange={(checked) => onChange(checked)}
					label={label}
					className="mt-1"
				/>
			);
			break;
		case "select":
			control = (
				<select
					{...common}
					value={typeof value === "string" ? value : ""}
					onChange={(event) => onChange(event.target.value || undefined)}
					className={cn(fieldClass, "h-11 sm:h-9")}
				>
					<option value="">{t("form.choose")}</option>
					{field.options.map((option) => (
						<option key={option.value} value={option.value}>
							{option.label}
						</option>
					))}
				</select>
			);
			break;
		case "multiSelect": {
			const picked = Array.isArray(value) ? (value as string[]) : [];
			control = (
				<div {...common} role="group" className="flex flex-wrap gap-1.5">
					{field.options.map((option) => {
						const on = picked.includes(option.value);
						return (
							<button
								key={option.value}
								type="button"
								role="checkbox"
								aria-checked={on}
								onClick={() =>
									onChange(
										on ? picked.filter((each) => each !== option.value) : [...picked, option.value],
									)
								}
								className={cn(
									"focus-visible:ring-ring/50 min-h-11 rounded-full border px-3 text-sm transition-colors duration-150 outline-none focus-visible:ring-[3px] sm:min-h-8",
									on
										? "border-primary/70 bg-primary/10 text-heading"
										: "border-border hover:bg-accent/60",
								)}
							>
								{option.label}
							</button>
						);
					})}
				</div>
			);
			break;
		}
		case "number":
		case "integer":
			control = (
				<input
					{...common}
					type="number"
					inputMode={field.kind === "integer" ? "numeric" : "decimal"}
					step={field.kind === "integer" ? 1 : "any"}
					min={field.minimum ?? undefined}
					max={field.maximum ?? undefined}
					value={typeof value === "number" || typeof value === "string" ? value : ""}
					onChange={(event) => onChange(event.target.value)}
					className={fieldClass}
				/>
			);
			break;
		case "text":
			control = (
				<input
					{...common}
					type={
						field.format === "email"
							? "email"
							: field.format === "uri"
								? "url"
								: field.format === "date"
									? "date"
									: field.format === "date-time"
										? "datetime-local"
										: "text"
					}
					value={typeof value === "string" ? value : ""}
					onChange={(event) => onChange(event.target.value)}
					className={fieldClass}
				/>
			);
			break;
		default:
			control = <p className="text-muted-foreground text-xs">{t("form.unsupported")}</p>;
	}
	return (
		<div className="flex flex-col gap-1.5">
			<label htmlFor={id} className="text-foreground text-sm font-medium">
				{label}
				{field.required ? (
					<span aria-hidden className="text-muted-foreground">
						{" "}
						*
					</span>
				) : null}
			</label>
			{field.description ? (
				<p id={hint} className="text-muted-foreground -mt-1 text-xs text-pretty">
					{field.description}
				</p>
			) : null}
			{control}
			{problem ? (
				<p id={error} className="text-destructive text-xs">
					{problem}
				</p>
			) : null}
		</div>
	);
}
