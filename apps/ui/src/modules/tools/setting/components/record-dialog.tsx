import { type ReactNode, useState } from "react";

import { kindLabel, KINDS } from "@/modules/tools/registry";
import {
	EMPTY_FORM,
	formFromRecord,
	type OutputFormat,
	parseRecord,
	type RecordForm,
	type RecordKind,
	recordErrors,
	recordFromForm,
	recordOver,
} from "@/modules/tools/setting/model";
import type { ToolSettingsRecordsItem } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";

const INPUT =
	"border-input bg-background focus-visible:ring-ring h-8 w-full rounded-md border px-2 text-sm outline-none focus-visible:ring-2";

/**
 * Adds or edits a tool of your own on a machine, in the registry's format
 * without a source: a form for the usual fields, or the record as JSON for
 * everything else (a JSON output's field mapping, settings).
 */
export function RecordDialog({
	open,
	onOpenChange,
	record,
	pending,
	error,
	onSave,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** The record being edited; a new one when absent. */
	record?: ToolSettingsRecordsItem;
	pending?: boolean;
	error?: string;
	onSave: (record: ToolSettingsRecordsItem) => void;
}) {
	const { t } = useTranslation("tools");
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="lg">
				<DialogHeader>
					<DialogTitle>
						{record ? t("recordDialog.editTitle") : t("recordDialog.addTitle")}
					</DialogTitle>
					<DialogDescription>{t("recordDialog.description")}</DialogDescription>
				</DialogHeader>
				{open && (
					<Editor
						key={String(record?.id ?? "")}
						record={record}
						pending={pending}
						error={error}
						onCancel={() => onOpenChange(false)}
						onSave={onSave}
					/>
				)}
			</DialogContent>
		</Dialog>
	);
}

function Editor({
	record,
	pending,
	error,
	onCancel,
	onSave,
}: {
	record?: ToolSettingsRecordsItem;
	pending?: boolean;
	error?: string;
	onCancel: () => void;
	onSave: (record: ToolSettingsRecordsItem) => void;
}) {
	const { t } = useTranslation(["tools", "common"]);
	const [mode, setMode] = useState<"form" | "json">("form");
	const [form, setForm] = useState<RecordForm>(record ? formFromRecord(record) : EMPTY_FORM);
	const [json, setJson] = useState(() =>
		JSON.stringify(record ?? recordFromForm(EMPTY_FORM), null, 2),
	);
	const [errors, setErrors] = useState<string[]>([]);
	const set = (patch: Partial<RecordForm>) => setForm((current) => ({ ...current, ...patch }));

	/** The form's record, over what the form does not show of the one edited. */
	const fromForm = () => recordOver(form, record);

	const switchTo = (next: "form" | "json") => {
		setErrors([]);
		if (next === "json") setJson(JSON.stringify(fromForm(), null, 2));
		else {
			const parsed = parseRecord(json);
			if (!parsed.record) return setErrors(parsed.errors);
			setForm(formFromRecord(parsed.record));
		}
		setMode(next);
	};

	const submit = () => {
		if (mode === "form") {
			const made = fromForm();
			const problems = recordErrors(made);
			setErrors(problems);
			if (problems.length === 0) onSave(made);
		} else {
			const parsed = parseRecord(json);
			setErrors(parsed.errors);
			if (parsed.record && parsed.errors.length === 0) onSave(parsed.record);
		}
	};

	const checks = form.kind === "linter" || form.kind === "check";

	return (
		<form
			className="flex min-h-0 flex-col"
			onSubmit={(event) => {
				event.preventDefault();
				submit();
			}}
		>
			<DialogBody className="gap-3">
				<div className="flex gap-1" role="group" aria-label={t("recordDialog.editAs")}>
					{(["form", "json"] as const).map((item) => (
						<Button
							key={item}
							type="button"
							size="xs"
							variant={mode === item ? "secondary" : "ghost"}
							aria-pressed={mode === item}
							onClick={() => mode !== item && switchTo(item)}
						>
							{item === "form" ? t("recordDialog.form") : "JSON"}
						</Button>
					))}
				</div>

				{mode === "form" ? (
					<div className="grid gap-3 sm:grid-cols-2">
						<Labeled label={t("recordDialog.fields.id")}>
							<input
								className={INPUT}
								value={form.id}
								placeholder="my-linter"
								onChange={(event) => set({ id: event.target.value })}
							/>
						</Labeled>
						<Labeled label={t("recordDialog.fields.name")}>
							<input
								className={INPUT}
								value={form.name}
								onChange={(event) => set({ name: event.target.value })}
							/>
						</Labeled>
						<Labeled label={t("recordDialog.fields.kind")}>
							<select
								className={INPUT}
								value={form.kind}
								onChange={(event) => set({ kind: event.target.value as RecordKind })}
							>
								{KINDS.map((kind) => (
									<option key={kind} value={kind}>
										{kindLabel(kind)}
									</option>
								))}
							</select>
						</Labeled>
						<Labeled label={t("recordDialog.fields.languages")}>
							<input
								className={INPUT}
								value={form.languages}
								placeholder="python, rust"
								onChange={(event) => set({ languages: event.target.value })}
							/>
						</Labeled>
						<Labeled label={t("recordDialog.fields.command")} wide>
							<input
								className={`${INPUT} font-mono`}
								value={form.command}
								placeholder="my-linter --format json"
								onChange={(event) => set({ command: event.target.value })}
							/>
						</Labeled>
						<Labeled label={t("recordDialog.fields.rootMarkers")} wide>
							<input
								className={INPUT}
								value={form.rootMarkers}
								placeholder="pyproject.toml, setup.cfg"
								onChange={(event) => set({ rootMarkers: event.target.value })}
							/>
						</Labeled>
						{checks && (
							<Labeled label={t("recordDialog.fields.output")}>
								<select
									className={INPUT}
									value={form.output}
									onChange={(event) => set({ output: event.target.value as OutputFormat })}
								>
									<option value="">{t("recordDialog.outputs.notSet")}</option>
									<option value="sarif">SARIF</option>
									<option value="checkstyle">Checkstyle XML</option>
									<option value="json">{t("recordDialog.outputs.json")}</option>
									<option value="pattern">{t("recordDialog.outputs.pattern")}</option>
								</select>
							</Labeled>
						)}
						{checks && form.output && (
							<Labeled label={t("recordDialog.fields.reports")}>
								<input
									className={`${INPUT} font-mono`}
									value={form.reports}
									placeholder={t("recordDialog.reportsPlaceholder")}
									onChange={(event) => set({ reports: event.target.value })}
								/>
							</Labeled>
						)}
						{checks && form.output === "pattern" && (
							<Labeled label={t("recordDialog.fields.pattern")}>
								<input
									className={`${INPUT} font-mono`}
									value={form.pattern}
									placeholder="(?<file>[^:]+):(?<line>\d+): (?<message>.*)"
									onChange={(event) => set({ pattern: event.target.value })}
								/>
							</Labeled>
						)}
					</div>
				) : (
					<label className="block space-y-1 text-sm">
						<span className="sr-only">{t("recordDialog.fields.record")}</span>
						<textarea
							aria-label={t("recordDialog.fields.record")}
							value={json}
							onChange={(event) => setJson(event.target.value)}
							spellCheck={false}
							rows={14}
							className="border-input bg-background focus-visible:ring-ring w-full rounded-md border p-2 font-mono text-xs outline-none focus-visible:ring-2"
						/>
					</label>
				)}

				{[...errors, ...(error ? [error] : [])].map((message) => (
					<p key={message} role="alert" className="text-destructive text-sm">
						{message}
					</p>
				))}
			</DialogBody>

			<DialogFooter>
				<Button type="button" variant="ghost" onClick={onCancel}>
					{t("common:actions.cancel")}
				</Button>
				<Button type="submit" disabled={pending}>
					{pending ? t("common:actions.saving") : t("common:actions.save")}
				</Button>
			</DialogFooter>
		</form>
	);
}

function Labeled({
	label,
	wide,
	children,
}: {
	label: string;
	wide?: boolean;
	children: ReactNode;
}) {
	return (
		<label className={`block space-y-1 text-sm ${wide ? "sm:col-span-2" : ""}`}>
			<span className="text-muted-foreground text-xs">{label}</span>
			{children}
		</label>
	);
}
