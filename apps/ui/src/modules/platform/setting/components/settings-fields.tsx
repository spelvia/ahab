import { useChangeSettings, useSettings } from "@/modules/platform/setting/api";
import type { SettingField } from "@/shared/api/generated/ahab.schemas";
import { t, useTranslation } from "@/shared/i18n";
import { Switch } from "@/shared/ui/switch";
import { toastError } from "@/shared/ui/toast";

/** The fields Ahab knows, which have names people read; others show their name. */
const LABELS = [
	"editableByDefault",
	"autoSave",
	"autoSaveDelay",
	"vim",
	"fontSize",
	"lineWrapping",
	"lineNumbers",
	"indentationGuides",
	"problemsAtLineEnd",
	"tabSize",
	"chatWhileWorking",
	"chatVim",
] as const;

const CHOICES = ["off", "afterDelay", "onFocusChange", "queue", "interrupt"] as const;

function labelOf(name: string): string {
	return (LABELS as readonly string[]).includes(name)
		? t(`platform:settingsFields.labels.${name as (typeof LABELS)[number]}`)
		: name;
}

function choiceText(choice: string): string {
	return (CHOICES as readonly string[]).includes(choice)
		? t(`platform:settingsFields.choices.${choice as (typeof CHOICES)[number]}`)
		: choice;
}

const select =
	"border-input bg-background focus-visible:ring-ring h-8 rounded-md border px-2 text-sm outline-none focus-visible:ring-2";

/**
 * A section's settings as a form: for the server, or (with `project`) the
 * fields a project may set, each "as the server says" until set.
 */
export function SettingsFields({
	section,
	project,
	only,
	except,
}: {
	section: string;
	project?: string;
	/** Only these fields, in the server's order (a page showing part of a section). */
	only?: readonly string[];
	/** All but these: the ones another page shows. */
	except?: readonly string[];
}) {
	// Renders again when the language changes; the labels read it through `t`.
	const { t: tp } = useTranslation("platform");
	const { t: tc } = useTranslation();
	const settings = useSettings(section, project);
	const change = useChangeSettings();
	if (settings.isPending)
		return <p className="text-muted-foreground text-sm">{tc("state.loading")}</p>;
	if (settings.isError || !settings.data)
		return <p className="text-muted-foreground text-sm">{tp("settingsFields.cannotLoad")}</p>;
	const { fields, values, server } = settings.data;
	const own = (project ? settings.data.project : server) ?? {};
	const shown = fields.filter(
		(field) =>
			(!project || field.perProject) &&
			(!only || only.includes(field.name)) &&
			!except?.includes(field.name),
	);
	const set = (name: string, value: unknown) =>
		change.mutate(
			{ section, params: project ? { project } : undefined, data: { [name]: value } },
			{ onError: toastError },
		);
	return (
		<ul className="border-border divide-border divide-y border-y">
			{shown.map((field) => (
				<li key={field.name} className="flex items-center gap-4 px-3 py-3.5">
					<div className="min-w-0 flex-1">
						<p className="text-heading text-sm font-semibold">{labelOf(field.name)}</p>
						<p className="text-muted-foreground text-sm text-pretty">{field.description}</p>
					</div>
					<Control
						field={field}
						value={project ? own[field.name] : values[field.name]}
						inherited={project ? (server[field.name] ?? field.default) : undefined}
						disabled={change.isPending}
						onChange={(value) => set(field.name, value)}
					/>
				</li>
			))}
		</ul>
	);
}

function shownValue(field: SettingField, value: unknown): string {
	if (field.kind === "boolean")
		return value ? t("platform:settingsFields.valueOn") : t("platform:settingsFields.valueOff");
	if (field.kind === "choice") return choiceText(String(value));
	return String(value);
}

function Control({
	field,
	value,
	inherited,
	disabled,
	onChange,
}: {
	field: SettingField;
	value: unknown;
	/** For a project: the value it has until set (the server's). */
	inherited?: unknown;
	disabled: boolean;
	onChange: (value: unknown) => void;
}) {
	const label = labelOf(field.name);
	if (inherited !== undefined) {
		// A project's: as the server says, or its own.
		const options =
			field.kind === "boolean"
				? [
						["true", t("platform:settingsFields.on")],
						["false", t("platform:settingsFields.off")],
					]
				: field.kind === "choice"
					? field.options.map((option) => [option, choiceText(option)])
					: [];
		if (field.kind === "integer")
			return (
				<input
					type="number"
					aria-label={label}
					min={field.minimum ?? undefined}
					max={field.maximum ?? undefined}
					placeholder={t("platform:settingsFields.serverPlaceholder", { value: String(inherited) })}
					defaultValue={value === undefined ? "" : String(value)}
					disabled={disabled}
					onBlur={(event) =>
						onChange(event.target.value === "" ? null : Number(event.target.value))
					}
					className={`${select} w-32`}
				/>
			);
		return (
			<select
				aria-label={label}
				value={value === undefined ? "" : String(value)}
				disabled={disabled}
				onChange={(event) => {
					const chosen = event.target.value;
					onChange(chosen === "" ? null : field.kind === "boolean" ? chosen === "true" : chosen);
				}}
				className={select}
			>
				<option value="">
					{t("platform:settingsFields.asServer", { value: shownValue(field, inherited) })}
				</option>
				{options.map(([option, text]) => (
					<option key={option} value={option}>
						{text}
					</option>
				))}
			</select>
		);
	}
	switch (field.kind) {
		case "boolean":
			return (
				<Switch
					checked={Boolean(value)}
					disabled={disabled}
					label={t(
						value ? "platform:settingsFields.switchOn" : "platform:settingsFields.switchOff",
						{ label },
					)}
					onCheckedChange={onChange}
				/>
			);
		case "choice":
			return (
				<select
					aria-label={label}
					value={String(value)}
					disabled={disabled}
					onChange={(event) => onChange(event.target.value)}
					className={select}
				>
					{field.options.map((option) => (
						<option key={option} value={option}>
							{choiceText(option)}
						</option>
					))}
				</select>
			);
		default:
			return (
				<input
					// Typed values are saved when the field is left.
					key={String(value)}
					type="number"
					aria-label={label}
					min={field.minimum ?? undefined}
					max={field.maximum ?? undefined}
					defaultValue={String(value)}
					disabled={disabled}
					onBlur={(event) => {
						const next = Number(event.target.value);
						if (event.target.value !== "" && next !== value) onChange(next);
					}}
					className={`${select} w-32`}
				/>
			);
	}
}
