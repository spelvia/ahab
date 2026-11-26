/**
 * Tool settings as the LSPs settings edit them, one layer at a
 * time: a tool's path, environment and off switch, a language's tools and
 * their order, when checks run, the environment, and your own records.
 * Every change leaves out what is empty, so a layer only holds what it sets.
 */

import type {
	EnvironmentChoice,
	LanguageChoice,
	ToolChoice,
	ToolSettings,
	ToolSettingsRecordsItem,
} from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";

// --- Extra environment -----------------------------------------------------

/** Variables as `KEY=value` lines, for a text box. */
export function envToText(env: Record<string, string> | undefined): string {
	return Object.entries(env ?? {})
		.map(([key, value]) => `${key}=${value}`)
		.join("\n");
}

const ENV_KEY = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** `KEY=value` lines back into variables; blank lines and `#` comments are skipped. */
export function parseEnv(text: string): { env: Record<string, string>; errors: string[] } {
	const env: Record<string, string> = {};
	const errors: string[] = [];
	for (const [index, raw] of text.split("\n").entries()) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const equals = line.indexOf("=");
		const key = equals === -1 ? line : line.slice(0, equals).trim();
		if (equals === -1 || !ENV_KEY.test(key)) {
			errors.push(t("tools:settingErrors.envLine", { line: index + 1 }));
			continue;
		}
		env[key] = line.slice(equals + 1).trim();
	}
	return { env, errors };
}

// --- Pruning ---------------------------------------------------------------

function isEmpty(value: unknown): boolean {
	if (value === undefined || value === null || value === "") return true;
	if (Array.isArray(value)) return false;
	return typeof value === "object" && Object.keys(value).length === 0;
}

function pruned<T extends object>(value: T): T {
	return Object.fromEntries(Object.entries(value).filter(([, item]) => !isEmpty(item))) as T;
}

function withEntry<T extends object>(
	map: Record<string, T> | undefined,
	key: string,
	value: T,
): Record<string, T> | undefined {
	const next = { ...map };
	const clean = pruned(value);
	if (isEmpty(clean)) delete next[key];
	else next[key] = clean;
	return isEmpty(next) ? undefined : next;
}

function withField<K extends keyof ToolSettings>(
	settings: ToolSettings,
	key: K,
	value: ToolSettings[K],
): ToolSettings {
	const next = { ...settings, [key]: value };
	if (isEmpty(value)) delete next[key];
	return next;
}

/** The settings with one tool's choices replaced; an empty choice is removed. */
export function withTool(settings: ToolSettings, id: string, choice: ToolChoice): ToolSettings {
	return withField(settings, "tools", withEntry(settings.tools, id, choice));
}

/** The settings with one language's choices replaced; an empty choice is removed. */
export function withLanguage(
	settings: ToolSettings,
	id: string,
	choice: LanguageChoice,
): ToolSettings {
	return withField(settings, "languages", withEntry(settings.languages, id, choice));
}

// --- Environment -----------------------------------------------------------

export type EnvironmentSwitch = "direnv" | "mise" | "nix";

/** Each switch's default: direnv and mise on, the Nix dev shell off (slow to start). */
const ENVIRONMENT_DEFAULTS: Record<EnvironmentSwitch, boolean> = {
	direnv: true,
	mise: true,
	nix: false,
};

export function environmentOn(
	environment: EnvironmentChoice | undefined,
	key: EnvironmentSwitch,
): boolean {
	return environment?.[key] ?? ENVIRONMENT_DEFAULTS[key];
}

/** The settings with a switch set; its default is stored as nothing. */
export function withEnvironment(
	settings: ToolSettings,
	patch: Partial<Record<EnvironmentSwitch, boolean>> & { env?: Record<string, string> },
): ToolSettings {
	const next: EnvironmentChoice = { ...settings.environment, ...patch };
	for (const key of Object.keys(ENVIRONMENT_DEFAULTS) as EnvironmentSwitch[])
		if (next[key] === ENVIRONMENT_DEFAULTS[key]) delete next[key];
	return withField(settings, "environment", isEmpty(pruned(next)) ? undefined : pruned(next));
}

// --- A language's tools ----------------------------------------------------

/** A language's tools in order: the project's when it sets them, else the defaults. */
export function toolsFor(
	defaults: string[],
	choice: LanguageChoice | undefined,
): { tools: string[]; custom: boolean } {
	return choice?.tools ? { tools: choice.tools, custom: true } : { tools: defaults, custom: false };
}

/** The list with the item at `index` moved by `delta`, kept in bounds. */
export function moved<T>(list: T[], index: number, delta: number): T[] {
	const to = index + delta;
	if (index < 0 || index >= list.length || to < 0 || to >= list.length) return list;
	const next = [...list];
	next.splice(to, 0, ...next.splice(index, 1));
	return next;
}

function sameList(a: string[], b: string[]): boolean {
	return a.length === b.length && a.every((item, index) => item === b[index]);
}

/** A language's new order, stored only when it differs from the defaults. */
export function withLanguageTools(
	settings: ToolSettings,
	language: string,
	tools: string[],
	defaults: string[],
): ToolSettings {
	const choice = settings.languages?.[language] ?? {};
	return withLanguage(settings, language, {
		...choice,
		tools: sameList(tools, defaults) ? undefined : tools,
	});
}

// --- When checks run -------------------------------------------------------

export type CheckWhen = "save" | "agentTurn" | "request";

/** When checks may run, in order; `tools:checkWhen.<id>` names each. */
export const CHECK_WHEN: CheckWhen[] = ["save", "agentTurn", "request"];

/** A check's times with one turned on or off; from the record's own when unset (`fallback`). */
export function toggledWhen(
	current: string[] | null | undefined,
	fallback: string[],
	when: CheckWhen,
	on: boolean,
): string[] {
	const base = current ?? fallback;
	const next = on ? [...new Set([...base, when])] : base.filter((item) => item !== when);
	return CHECK_WHEN.filter((id) => next.includes(id));
}

// --- Your own tools --------------------------------------------------------

export type RecordKind = "languageServer" | "formatter" | "linter" | "check";
export type OutputFormat = "" | "sarif" | "checkstyle" | "json" | "pattern";
const FORMATS = new Set<unknown>(["sarif", "checkstyle", "json", "pattern"]);

/** A record of your own as the form shows it. */
export interface RecordForm {
	id: string;
	name: string;
	kind: RecordKind;
	/** Language IDs, separated by commas. */
	languages: string;
	/** The command as typed; quotes keep words together. */
	command: string;
	/** Separated by commas. */
	rootMarkers: string;
	/** A linter's or check's output. */
	output: OutputFormat;
	/** The regular expression, for `pattern`. */
	pattern: string;
	/** Report files to read instead of its output, separated by commas. */
	reports: string;
}

export const EMPTY_FORM: RecordForm = {
	id: "",
	name: "",
	kind: "linter",
	languages: "",
	command: "",
	rootMarkers: "",
	output: "",
	pattern: "",
	reports: "",
};

/** A command line split into words, as a shell would for plain quoting. */
export function splitCommand(text: string): string[] {
	const words: string[] = [];
	for (const match of text.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g))
		words.push(match[1] ?? match[2] ?? match[3] ?? "");
	return words;
}

function joinCommand(words: string[]): string {
	return words.map((word) => (/\s/.test(word) ? `"${word}"` : word)).join(" ");
}

const list = (text: string) =>
	text
		.split(",")
		.map((item) => item.trim())
		.filter(Boolean);

const TOOL_ID = /^[a-z0-9][a-z0-9-]*$/;
const KIND_IDS = new Set<unknown>(["languageServer", "formatter", "linter", "check"]);
const isKind = (value: unknown): value is RecordKind => KIND_IDS.has(value);

type Json = { [key: string]: unknown };
const isObject = (value: unknown): value is Json =>
	typeof value === "object" && value !== null && !Array.isArray(value);

/** What is wrong with a record of your own; nothing when it can be saved. */
export function recordErrors(record: Json): string[] {
	const errors: string[] = [];
	const run = isObject(record.run) ? record.run : {};
	if (typeof record.id !== "string" || !TOOL_ID.test(record.id))
		errors.push(t("tools:settingErrors.id"));
	if (typeof record.name !== "string" || !record.name.trim())
		errors.push(t("tools:settingErrors.name"));
	if (!isKind(record.kind)) errors.push(t("tools:settingErrors.kind"));
	if (
		!Array.isArray(record.languages) ||
		record.languages.length === 0 ||
		!record.languages.every((item) => typeof item === "string")
	)
		errors.push(t("tools:settingErrors.languages"));
	if (
		!Array.isArray(run.command) ||
		run.command.length === 0 ||
		!run.command.every((item) => typeof item === "string")
	)
		errors.push(t("tools:settingErrors.command"));
	if ("source" in record) errors.push(t("tools:settingErrors.source"));
	return errors;
}

/** The record the form describes. */
export function recordFromForm(form: RecordForm): Json {
	const run: Json = { command: splitCommand(form.command) };
	const markers = list(form.rootMarkers);
	if (markers.length > 0) run.rootMarkers = markers;
	if ((form.kind === "linter" || form.kind === "check") && form.output) {
		const output: Json = { format: form.output };
		if (form.output === "pattern") output.pattern = { regexp: form.pattern };
		const reports = list(form.reports);
		if (reports.length > 0) output.reports = reports;
		run.output = output;
	}
	return {
		id: form.id.trim(),
		name: form.name.trim(),
		kind: form.kind,
		languages: list(form.languages),
		run,
	};
}

/**
 * The form's record over the one edited: what the form does not show (a
 * JSON output's field mapping, settings, environment) is kept.
 */
export function recordOver(form: RecordForm, base?: Json): Json {
	const made = recordFromForm(form);
	if (!base) return made;
	const baseRun = isObject(base.run) ? base.run : {};
	const madeRun = made.run as Json;
	const run: Json = { ...baseRun, ...madeRun };
	if (!("rootMarkers" in madeRun)) delete run.rootMarkers;
	const old = isObject(baseRun.output) ? baseRun.output : undefined;
	const now = isObject(madeRun.output) ? madeRun.output : undefined;
	if (!now) delete run.output;
	else if (old && old.format === now.format) {
		const kept: Json =
			now.format === "pattern"
				? {
						...old,
						pattern: { ...(isObject(old.pattern) ? old.pattern : {}), regexp: form.pattern },
					}
				: { ...old };
		if (now.reports) kept.reports = now.reports;
		else delete kept.reports;
		run.output = kept;
	}
	return { ...base, ...made, run };
}

const strings = (value: unknown) =>
	Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];

/** A record in the form; what the form cannot show stays in the JSON. */
export function formFromRecord(record: Json): RecordForm {
	const run = isObject(record.run) ? record.run : {};
	const output = isObject(run.output) ? run.output : {};
	const pattern = isObject(output.pattern) ? output.pattern : {};
	const format = output.format;
	return {
		id: typeof record.id === "string" ? record.id : "",
		name: typeof record.name === "string" ? record.name : "",
		kind: isKind(record.kind) ? record.kind : "linter",
		languages: strings(record.languages).join(", "),
		command: joinCommand(strings(run.command)),
		rootMarkers: strings(run.rootMarkers).join(", "),
		output: FORMATS.has(format) ? (format as OutputFormat) : "",
		pattern: typeof pattern.regexp === "string" ? pattern.regexp : "",
		reports: strings(output.reports).join(", "),
	};
}

/** A record typed as JSON, or why it cannot be read. */
export function parseRecord(text: string): { record?: Json; errors: string[] } {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch (error) {
		return {
			errors: [
				t("tools:settingErrors.notJson", {
					message: error instanceof Error ? error.message : String(error),
				}),
			],
		};
	}
	if (!isObject(value)) return { errors: [t("tools:settingErrors.notObject")] };
	return { record: value, errors: recordErrors(value) };
}

/** The settings with a record added, or put in the place of `replacing` (its old ID). */
export function withRecord(
	settings: ToolSettings,
	record: ToolSettingsRecordsItem,
	replacing?: string,
): ToolSettings {
	const records = [...(settings.records ?? [])];
	const index = records.findIndex((item) => item.id === (replacing ?? record.id));
	if (index === -1) records.push(record);
	else records[index] = record;
	return { ...settings, records };
}

export function withoutRecord(settings: ToolSettings, id: string): ToolSettings {
	const records = (settings.records ?? []).filter((item) => item.id !== id);
	return withField(settings, "records", records.length > 0 ? records : undefined);
}

/** The default of `maxServers` on a machine. */
export const MAX_SERVERS = 8;

/** The default of `maxChecks` on a machine. */
export const MAX_CHECKS = 2;

/** The default of `activeDays` in a project. */
export const ACTIVE_DAYS = 14;
