import { cn } from "cn";
import { useMemo, useState } from "react";

import { useMachines } from "@/modules/machines";
import { useInherited, useSaveSettings, useScopedSettings } from "@/modules/plugins/setting/api";
import { useProjects } from "@/modules/projects";
import type { PluginSettingsScope } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

interface Field {
	type?: string;
	enum?: unknown[];
	default?: unknown;
	description?: string;
	minimum?: number;
	maximum?: number;
}

const SCOPES: PluginSettingsScope[] = ["server", "machine", "project"];

/** `logTailLines` as "Log tail lines". */
function title(name: string): string {
	const words = name
		.replace(/([a-z0-9])([A-Z])/g, "$1 $2")
		.replaceAll(/[_-]/g, " ")
		.toLowerCase();
	return words.charAt(0).toUpperCase() + words.slice(1);
}

function shown(value: unknown): string {
	if (value === undefined || value === null) return "";
	if (Array.isArray(value)) return value.join(", ");
	if (typeof value === "object") return JSON.stringify(value);
	return String(value);
}

/** What an input's text is as a value. */
function parsed(field: Field, text: string): unknown {
	if (field.type === "number" || field.type === "integer") return Number(text);
	if (field.type === "array")
		return text
			.split(",")
			.map((item) => item.trim())
			.filter(Boolean);
	if (field.type === "object") {
		try {
			return JSON.parse(text);
		} catch {
			return undefined;
		}
	}
	return text;
}

const input =
	"border-input bg-background focus-visible:ring-ring h-8 w-full max-w-sm rounded-md border px-2.5 text-sm outline-none focus-visible:ring-2";

/**
 * A plugin's settings at a scope, as label and value rows: each field says
 * what it inherits when not set here, and can be reset to inherit again.
 */
export function SettingsForm({
	plugin,
	simpleOnly,
}: {
	plugin: string;
	/** Leave out fields that are objects: the plugin's own page sets those. */
	simpleOnly?: boolean;
}) {
	const { t } = useTranslation("plugins");
	const { t: tc } = useTranslation();
	const [scope, setScope] = useState<PluginSettingsScope>("server");
	const [target, setTarget] = useState<string | undefined>();
	const machines = useMachines();
	const projects = useProjects();
	const targets = useMemo(() => {
		if (scope === "machine")
			return (machines.data?.items ?? []).map((machine) => ({
				id: machine.id,
				name: machine.name,
			}));
		if (scope === "project")
			return (projects.data?.items ?? []).map((project) => ({
				id: project.id,
				name: `${project.name} · ${project.machineName}`,
			}));
		return [];
	}, [scope, machines.data, projects.data]);
	const at = scope === "server" ? undefined : (target ?? targets[0]?.id);
	const settings = useScopedSettings(plugin, scope, at);
	const inherited = useInherited(
		plugin,
		scope === "machine" ? { machine: at } : scope === "project" ? { project: at } : {},
	);
	const save = useSaveSettings();
	const fields = Object.entries(
		((settings.data?.schema as { properties?: Record<string, Field> } | undefined)?.properties ??
			{}) as Record<string, Field>,
	).filter(([, field]) => !simpleOnly || field.type !== "object");
	const saved = (settings.data?.values ?? {}) as Record<string, unknown>;
	const wider = (inherited.data?.values ?? {}) as Record<string, unknown>;
	// What is set here: saved values with the edits over them (undefined: inherit).
	const [edits, setEdits] = useState<Record<string, unknown>>({});
	const here = (name: string) => (name in edits ? edits[name] : saved[name]);
	const dirty = Object.keys(edits).length > 0;
	const change = (name: string, value: unknown) => setEdits((now) => ({ ...now, [name]: value }));
	const reset = () => setEdits({});
	// Where a field's value comes from when not set here, with that value when shown.
	const sourceText = (fromWider: boolean, value: string | undefined) =>
		value === undefined
			? t(fromWider ? "settingsForm.fromWider" : "settingsForm.default")
			: t(fromWider ? "settingsForm.fromWiderValue" : "settingsForm.defaultValue", { value });
	const pick = (next: PluginSettingsScope) => {
		setScope(next);
		setTarget(undefined);
		reset();
	};

	return (
		<div className="space-y-4">
			<div className="flex flex-wrap items-center gap-2">
				<div
					role="radiogroup"
					aria-label={t("settingsForm.scopes.label")}
					className="bg-muted inline-flex rounded-lg p-0.5"
				>
					{SCOPES.map((item) => (
						<button
							key={item}
							type="button"
							role="radio"
							aria-checked={scope === item}
							onClick={() => pick(item)}
							className={cn(
								"rounded-md px-3 py-1 text-sm",
								scope === item
									? "bg-background text-heading shadow-sm"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							{t(`settingsForm.scopes.${item}`)}
						</button>
					))}
				</div>
				{scope === "server" ? (
					<span className="text-muted-foreground text-xs">{t("settingsForm.serverNote")}</span>
				) : (
					<select
						aria-label={t(`settingsForm.which.${scope}`)}
						value={at ?? ""}
						onChange={(event) => {
							setTarget(event.target.value);
							reset();
						}}
						className={cn(input, "w-auto min-w-48")}
					>
						{targets.map((item) => (
							<option key={item.id} value={item.id}>
								{item.name}
							</option>
						))}
					</select>
				)}
			</div>

			<dl className="border-border divide-border divide-y border-y text-sm">
				{fields.map(([name, field]) => {
					const value = here(name);
					const set = value !== undefined;
					const fallback = wider[name] ?? field.default;
					const effective = set ? value : fallback;
					return (
						<div key={name} className="grid gap-2 py-3 sm:grid-cols-[16rem_1fr] sm:gap-4">
							<dt className="min-w-0">
								<span className="text-foreground font-medium">{title(name)}</span>
								{field.description ? (
									<span className="text-muted-foreground mt-0.5 block text-xs">
										{field.description}
									</span>
								) : null}
							</dt>
							<dd className="flex min-w-0 flex-wrap items-center gap-3">
								{field.type === "boolean" ? (
									<Switch
										label={title(name)}
										checked={Boolean(effective)}
										onCheckedChange={(on) => change(name, on)}
									/>
								) : field.enum ? (
									<select
										aria-label={title(name)}
										value={shown(effective)}
										onChange={(event) => change(name, event.target.value)}
										className={cn(input, "w-auto")}
									>
										{field.enum.map((option) => (
											<option key={String(option)} value={String(option)}>
												{String(option)}
											</option>
										))}
									</select>
								) : field.type === "object" ? (
									<textarea
										aria-label={title(name)}
										value={set ? shown(value) : ""}
										placeholder={shown(fallback)}
										onChange={(event) =>
											change(
												name,
												event.target.value ? parsed(field, event.target.value) : undefined,
											)
										}
										rows={3}
										className={cn(input, "h-auto py-1.5 font-mono text-xs")}
									/>
								) : (
									<input
										aria-label={title(name)}
										value={set ? shown(value) : ""}
										placeholder={shown(fallback)}
										inputMode={
											field.type === "number" || field.type === "integer" ? "decimal" : undefined
										}
										onChange={(event) =>
											change(
												name,
												event.target.value === "" ? undefined : parsed(field, event.target.value),
											)
										}
										className={input}
									/>
								)}
								{set ? (
									<span className="text-muted-foreground flex items-center gap-2 text-xs">
										<span className="pill pill-ok">{t("settingsForm.setHere")}</span>
										<button
											type="button"
											className="hover:text-foreground underline underline-offset-4"
											onClick={() => change(name, undefined)}
										>
											{t("settingsForm.reset")}
										</button>
									</span>
								) : (
									<span className="text-muted-foreground text-xs">
										{sourceText(
											name in wider && wider[name] !== field.default,
											field.type === "boolean" || field.enum
												? undefined
												: shown(fallback) || t("settingsForm.none"),
										)}
									</span>
								)}
							</dd>
						</div>
					);
				})}
			</dl>

			{save.error ? (
				<p role="alert" className="text-destructive text-sm">
					{save.error.message}
				</p>
			) : null}
			{dirty ? (
				<div className="flex items-center gap-2">
					<Button
						size="sm"
						disabled={save.isPending}
						onClick={() => {
							const values: Record<string, unknown> = {};
							for (const [name] of fields) {
								const value = here(name);
								if (value !== undefined) values[name] = value;
							}
							save.mutate(
								{ id: plugin, params: { scope, id: at }, data: { values } },
								{ onSuccess: reset },
							);
						}}
					>
						{save.isPending ? tc("actions.saving") : t("settingsForm.save")}
					</Button>
					<Button size="sm" variant="ghost" onClick={reset}>
						{t("settingsForm.discard")}
					</Button>
				</div>
			) : null}
		</div>
	);
}
