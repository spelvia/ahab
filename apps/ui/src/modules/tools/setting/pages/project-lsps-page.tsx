import { ArrowDown, ArrowUp, ChevronRight, X } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { useFile, useProject } from "@/modules/projects";
import {
	kindLabel,
	languageNamer,
	type ToolLanguage,
	toolLanguages,
	useMachineTools,
} from "@/modules/tools/registry";
import { useProjectToolSettings, useSaveProjectToolSettings } from "@/modules/tools/setting/api";
import {
	ACTIVE_DAYS,
	CHECK_WHEN,
	envToText,
	type EnvironmentSwitch,
	environmentOn,
	moved,
	parseEnv,
	toggledWhen,
	toolsFor,
	withEnvironment,
	withLanguage,
	withLanguageTools,
	withTool,
} from "@/modules/tools/setting/model";
import type {
	Project,
	ServerStart,
	ToolSettings,
	ToolStatusDto,
} from "@/shared/api/generated/ahab.schemas";
import { formatList, Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

const SELECT =
	"border-input bg-background focus-visible:ring-ring h-7 rounded-md border px-1.5 text-xs outline-none focus-visible:ring-2";

/** Where a repository shares its tool choices with everyone who opens it. */
const REPOSITORY_FILE = ".ahab/tools.json";

/**
 * Settings → Projects → a project → LSPs (its header and tabs are
 * the project's): the project's choices: each language's tools and their
 * order, formatting, the environment, and when checks run.
 */
export function ProjectLspsPage({ projectId }: { projectId: string }) {
	const { t } = useTranslation("tools");
	const project = useProject(projectId);
	return (
		<section className="max-w-3xl space-y-8">
			<p className="text-muted-foreground text-sm">
				{project.data
					? t("projectLsps.intro", { project: project.data.name })
					: t("projectLsps.introUnnamed")}
			</p>
			{project.isPending && (
				<p className="text-muted-foreground text-sm">{t("projectLsps.loadingProject")}</p>
			)}
			{project.isError && (
				<p role="alert" className="text-destructive text-sm">
					{t("projectLsps.projectError", { message: project.error.message })}
				</p>
			)}
			{project.data && <Loaded project={project.data} />}
		</section>
	);
}

function Loaded({ project }: { project: Project }) {
	const { t } = useTranslation(["tools", "common"]);
	const settings = useProjectToolSettings(project.id);
	const saving = useSaveProjectToolSettings(project.id);
	const available = project.availability === "available";
	const registry = useMachineTools(project.machineId, available);
	const repository = useFile(project.id, REPOSITORY_FILE, available);
	const languages = useMemo(() => toolLanguages(), []);
	const nameOf = useMemo(() => languageNamer(), []);
	const tools = registry.data?.tools ?? [];

	if (settings.isPending)
		return <p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>;
	if (settings.isError)
		return (
			<p role="alert" className="text-destructive text-sm">
				{t("projectLsps.settingsError", { message: settings.error.message })}
			</p>
		);
	const current = settings.data;
	const save = (next: ToolSettings) => saving.save(next);
	const shared = Boolean(project && repository.data && !repository.isError);

	return (
		<div className="space-y-8">
			{shared && project && (
				<p className="border-border bg-muted/40 rounded-md border px-3 py-2 text-sm">
					<Trans
						t={t}
						i18nKey="projectLsps.shared"
						values={{ file: REPOSITORY_FILE }}
						components={{ code: <code className="font-mono text-xs" /> }}
					/>
				</p>
			)}
			{!available && <p className="text-muted-foreground text-sm">{t("projectLsps.noMachine")}</p>}
			{saving.isError && (
				<p role="alert" className="text-destructive text-sm">
					{t("projectLsps.saveError", { message: saving.error.message })}
				</p>
			)}

			<Repositories settings={current} pending={saving.isPending} onSave={save} />
			<Environment settings={current} pending={saving.isPending} onSave={save} />
			<Languages
				languages={languages}
				tools={tools}
				settings={current}
				pending={saving.isPending}
				onSave={save}
			/>
			<Checks
				tools={tools}
				nameOf={nameOf}
				settings={current}
				pending={saving.isPending}
				onSave={save}
			/>
			<TurnedOff tools={tools} settings={current} pending={saving.isPending} onSave={save} />
		</div>
	);
}

function Section({
	id,
	title,
	description,
	children,
}: {
	/** The heading's ID, the same in every language. */
	id: string;
	title: string;
	description?: string;
	children: ReactNode;
}) {
	return (
		<section aria-labelledby={id} className="space-y-3">
			<div className="space-y-0.5">
				<h2 id={id} className="text-base font-bold">
					{title}
				</h2>
				{description && <p className="text-muted-foreground text-sm">{description}</p>}
			</div>
			{children}
		</section>
	);
}

/** When servers start; `tools:projectLsps.repositories.starts.<id>` names each. */
const STARTS = ["open", "pinned", "off"] as const satisfies readonly ServerStart[];

/**
 * A project of many repositories: when language servers start,
 * and how long a repository stays active after it was used.
 */
function Repositories({
	settings,
	pending,
	onSave,
}: {
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const [days, setDays] = useState(String(settings.activeDays ?? ACTIVE_DAYS));
	const parsed = Number.parseInt(days, 10);
	const valid = Number.isInteger(parsed) && parsed >= 1 && parsed <= 365;
	const changed = valid && parsed !== (settings.activeDays ?? ACTIVE_DAYS);
	const { t } = useTranslation(["tools", "common"]);
	return (
		<Section
			id="lsps-repositories"
			title={t("projectLsps.repositories.title")}
			description={t("projectLsps.repositories.description")}
		>
			<ul className="border-border divide-border divide-y border-y text-sm">
				<li className="flex flex-wrap items-center justify-between gap-3 py-2.5">
					<label htmlFor="servers-start" className="space-y-0.5">
						<span className="block">{t("projectLsps.repositories.serversStart")}</span>
						<span className="text-muted-foreground block text-xs">
							{t("projectLsps.repositories.serversStartHelp")}
						</span>
					</label>
					<select
						id="servers-start"
						className={SELECT}
						disabled={pending}
						value={settings.servers ?? "open"}
						onChange={(event) => {
							const next = event.target.value as ServerStart;
							onSave({ ...settings, servers: next === "open" ? null : next });
						}}
					>
						{STARTS.map((start) => (
							<option key={start} value={start}>
								{t(`projectLsps.repositories.starts.${start}`)}
							</option>
						))}
					</select>
				</li>
				<li className="flex flex-wrap items-center justify-between gap-3 py-2.5">
					<label htmlFor="active-days" className="space-y-0.5">
						<span className="block">{t("projectLsps.repositories.activeDays")}</span>
						<span className="text-muted-foreground block text-xs">
							{t("projectLsps.repositories.activeDaysHelp", { value: ACTIVE_DAYS })}
						</span>
					</label>
					<span className="flex items-center gap-2">
						<input
							id="active-days"
							type="number"
							min={1}
							max={365}
							value={days}
							onChange={(event) => setDays(event.target.value)}
							className="border-input bg-background focus-visible:ring-ring h-8 w-20 rounded-md border px-2 text-sm tabular-nums outline-none focus-visible:ring-2"
						/>
						{changed && (
							<Button
								size="sm"
								disabled={pending}
								onClick={() =>
									onSave({ ...settings, activeDays: parsed === ACTIVE_DAYS ? null : parsed })
								}
							>
								{t("common:actions.save")}
							</Button>
						)}
					</span>
				</li>
			</ul>
		</Section>
	);
}

/**
 * Sources turned off that are not tools listed above: a plugin's
 * collections of problems (`plugin.collection`), turned off from Problems.
 */
function TurnedOff({
	tools,
	settings,
	pending,
	onSave,
}: {
	tools: ToolStatusDto[];
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const { t } = useTranslation("tools");
	const off = Object.entries(settings.tools ?? {})
		.filter(([id, choice]) => choice.off && !tools.some((tool) => tool.id === id))
		.map(([id]) => id);
	if (!off.length) return null;
	return (
		<Section
			id="lsps-problems-turned-off"
			title={t("projectLsps.turnedOff.title")}
			description={t("projectLsps.turnedOff.description")}
		>
			<ul className="border-border divide-border divide-y border-y text-sm">
				{off.map((id) => (
					<li key={id} className="flex items-center justify-between gap-3 py-2">
						<code className="font-mono text-xs">{id}</code>
						<Button
							size="xs"
							variant="ghost"
							disabled={pending}
							onClick={() => onSave(withTool(settings, id, { ...settings.tools?.[id], off: null }))}
						>
							{t("projectLsps.turnedOff.turnOn")}
						</Button>
					</li>
				))}
			</ul>
		</Section>
	);
}

const SWITCHES: EnvironmentSwitch[] = ["direnv", "mise", "nix"];

function Environment({
	settings,
	pending,
	onSave,
}: {
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const { t } = useTranslation(["tools", "common"]);
	const switches = SWITCHES.map((id) => ({
		id,
		// direnv is its name, the same in every language.
		label:
			id === "direnv"
				? "direnv"
				: id === "mise"
					? t("projectLsps.environment.mise")
					: t("projectLsps.environment.nix"),
		detail:
			id === "direnv"
				? t("projectLsps.environment.direnvHelp")
				: id === "mise"
					? t("projectLsps.environment.miseHelp")
					: t("projectLsps.environment.nixHelp"),
	}));
	const saved = envToText(settings.environment?.env);
	const [env, setEnv] = useState(saved);
	const [errors, setErrors] = useState<string[]>([]);
	return (
		<Section
			id="lsps-environment"
			title={t("projectLsps.environment.title")}
			description={t("projectLsps.environment.description")}
		>
			<ul className="border-border divide-border divide-y border-y text-sm">
				{switches.map((item) => (
					<li key={item.id} className="flex items-center justify-between gap-4 py-2.5">
						<span className="space-y-0.5">
							<span className="block">{item.label}</span>
							<span className="text-muted-foreground block text-xs">{item.detail}</span>
						</span>
						<Switch
							label={item.label}
							checked={environmentOn(settings.environment, item.id)}
							disabled={pending}
							onCheckedChange={(next) => onSave(withEnvironment(settings, { [item.id]: next }))}
						/>
					</li>
				))}
				<li className="space-y-1.5 py-2.5">
					<label htmlFor="project-env" className="block">
						{t("projectLsps.environment.extra")}
						<span className="text-muted-foreground block text-xs">
							{t("projectLsps.environment.extraHelp")}
						</span>
					</label>
					<textarea
						id="project-env"
						rows={2}
						value={env}
						spellCheck={false}
						onChange={(event) => setEnv(event.target.value)}
						className="border-input bg-background focus-visible:ring-ring w-full rounded-md border px-2 py-1.5 font-mono text-xs outline-none focus-visible:ring-2"
					/>
					{errors.map((message) => (
						<p key={message} role="alert" className="text-destructive text-xs">
							{message}
						</p>
					))}
					{env !== saved && (
						<Button
							size="sm"
							disabled={pending}
							onClick={() => {
								const parsed = parseEnv(env);
								setErrors(parsed.errors);
								if (parsed.errors.length === 0)
									onSave(withEnvironment(settings, { env: parsed.env }));
							}}
						>
							{t("common:actions.save")}
						</Button>
					)}
				</li>
			</ul>
		</Section>
	);
}

function Languages({
	languages,
	tools,
	settings,
	pending,
	onSave,
}: {
	languages: ToolLanguage[];
	tools: ToolStatusDto[];
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const shown = languages
		.filter(
			(language) =>
				language.tools.length > 0 ||
				settings.languages?.[language.id] ||
				tools.some((tool) => tool.languages.includes(language.id)),
		)
		.toSorted((a, b) => a.name.localeCompare(b.name));
	const { t } = useTranslation("tools");
	return (
		<Section
			id="lsps-languages"
			title={t("projectLsps.languages.title")}
			description={t("projectLsps.languages.description")}
		>
			<ul className="border-border divide-border divide-y border-y text-sm">
				{shown.map((language) => (
					<LanguageRow
						key={language.id}
						language={language}
						tools={tools}
						settings={settings}
						pending={pending}
						onSave={onSave}
					/>
				))}
			</ul>
		</Section>
	);
}

function LanguageRow({
	language,
	tools,
	settings,
	pending,
	onSave,
}: {
	language: ToolLanguage;
	tools: ToolStatusDto[];
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const { t } = useTranslation("tools");
	const [open, setOpen] = useState(false);
	const choice = settings.languages?.[language.id] ?? {};
	const order = toolsFor(language.tools, choice);
	const off = Boolean(choice.off);
	const named = (id: string) => tools.find((tool) => tool.id === id);
	const label = (id: string) => named(id)?.name ?? id;
	const addable = tools.filter(
		(tool) => tool.languages.includes(language.id) && !order.tools.includes(tool.id),
	);
	const formatters = order.tools.filter((id) => {
		const kind = named(id)?.kind;
		return !kind || kind === "formatter" || kind === "languageServer";
	});
	const setTools = (next: string[]) =>
		onSave(withLanguageTools(settings, language.id, next, language.tools));
	const set = (patch: typeof choice) =>
		onSave(withLanguage(settings, language.id, { ...choice, ...patch }));
	const summary = off
		? t("projectLsps.languages.off")
		: order.tools.length > 0
			? formatList(order.tools.map(label), "unit")
			: t("projectLsps.languages.noTools");

	return (
		<li className="py-2">
			<div className="flex items-center gap-3">
				<button
					type="button"
					aria-expanded={open}
					onClick={() => setOpen(!open)}
					className="flex min-w-0 flex-1 items-center gap-2 text-left"
				>
					<ChevronRight
						className={`size-3.5 shrink-0 transition-transform ${open ? "rotate-90" : ""}`}
					/>
					<span className={off ? "text-muted-foreground" : "text-heading font-medium"}>
						{language.name}
					</span>
					<span className="text-muted-foreground truncate text-xs">
						{!order.custom && order.tools.length > 0
							? t("projectLsps.languages.byDefault", { summary })
							: summary}
					</span>
				</button>
				<Switch
					label={t("projectLsps.languages.tools", { language: language.name })}
					checked={!off}
					disabled={pending}
					onCheckedChange={(next) => set({ off: next ? null : true })}
				/>
			</div>
			{open && !off && (
				<div className="space-y-3 pt-2 pb-1 pl-5">
					<ol
						className="space-y-1"
						aria-label={t("projectLsps.languages.tools", { language: language.name })}
					>
						{order.tools.map((id, index) => (
							<li key={id} className="flex items-center gap-2">
								<span className="text-muted-foreground w-4 text-xs tabular-nums">{index + 1}</span>
								<span className="min-w-0 flex-1">
									{label(id)}
									{named(id) && (
										<span className="text-muted-foreground text-xs">
											{" "}
											· {kindLabel(named(id)?.kind ?? "")}
										</span>
									)}
								</span>
								<Button
									size="icon-xs"
									variant="ghost"
									aria-label={t("projectLsps.languages.moveUp", { tool: label(id) })}
									disabled={pending || index === 0}
									onClick={() => setTools(moved(order.tools, index, -1))}
								>
									<ArrowUp />
								</Button>
								<Button
									size="icon-xs"
									variant="ghost"
									aria-label={t("projectLsps.languages.moveDown", { tool: label(id) })}
									disabled={pending || index === order.tools.length - 1}
									onClick={() => setTools(moved(order.tools, index, 1))}
								>
									<ArrowDown />
								</Button>
								<Button
									size="icon-xs"
									variant="ghost"
									aria-label={t("projectLsps.languages.remove", { tool: label(id) })}
									disabled={pending}
									onClick={() => setTools(order.tools.filter((item) => item !== id))}
								>
									<X />
								</Button>
							</li>
						))}
					</ol>
					<div className="flex flex-wrap items-center gap-2 text-xs">
						{addable.length > 0 && (
							<select
								aria-label={t("projectLsps.languages.addTo", { language: language.name })}
								className={SELECT}
								value=""
								disabled={pending}
								onChange={(event) =>
									event.target.value && setTools([...order.tools, event.target.value])
								}
							>
								<option value="">{t("projectLsps.languages.add")}</option>
								{addable.map((tool) => (
									<option key={tool.id} value={tool.id}>
										{t("projectLsps.languages.toolWithKind", {
											tool: tool.name,
											kind: kindLabel(tool.kind),
										})}
									</option>
								))}
							</select>
						)}
						<label className="flex items-center gap-1.5">
							<span className="text-muted-foreground">
								{t("projectLsps.languages.formatOnSave")}
							</span>
							<select
								className={SELECT}
								value={choice.formatOnSave == null ? "" : String(choice.formatOnSave)}
								disabled={pending}
								onChange={(event) =>
									set({
										formatOnSave: event.target.value === "" ? null : event.target.value === "true",
									})
								}
							>
								<option value="">{t("projectLsps.languages.asEditor")}</option>
								<option value="true">{t("projectLsps.languages.on")}</option>
								<option value="false">{t("projectLsps.languages.offOption")}</option>
							</select>
						</label>
						<label className="flex items-center gap-1.5">
							<span className="text-muted-foreground">{t("projectLsps.languages.formatter")}</span>
							<select
								className={SELECT}
								value={choice.formatter ?? ""}
								disabled={pending}
								onChange={(event) => set({ formatter: event.target.value || null })}
							>
								<option value="">{t("projectLsps.languages.firstThatCan")}</option>
								{formatters.map((id) => (
									<option key={id} value={id}>
										{label(id)}
									</option>
								))}
							</select>
						</label>
						{order.custom && (
							<Button
								size="xs"
								variant="ghost"
								disabled={pending}
								onClick={() => setTools(language.tools)}
							>
								{t("projectLsps.languages.backToDefaults")}
							</Button>
						)}
					</div>
				</div>
			)}
		</li>
	);
}

function Checks({
	tools,
	nameOf,
	settings,
	pending,
	onSave,
}: {
	tools: ToolStatusDto[];
	nameOf: (id: string) => string;
	settings: ToolSettings;
	pending: boolean;
	onSave: (settings: ToolSettings) => void;
}) {
	const { t } = useTranslation("tools");
	const checks = tools.filter((tool) => tool.kind === "check");
	if (checks.length === 0) return null;
	return (
		<Section
			id="lsps-checks"
			title={t("projectLsps.checks.title")}
			description={t("projectLsps.checks.description")}
		>
			<ul className="border-border divide-border divide-y border-y text-sm">
				{checks.map((tool) => {
					const choice = settings.tools?.[tool.id] ?? {};
					const off = Boolean(choice.off);
					return (
						<li key={tool.id} className="flex flex-wrap items-center gap-3 py-2.5">
							<span className="min-w-0 flex-1">
								<span className={off ? "text-muted-foreground" : "text-heading font-medium"}>
									{tool.name}
								</span>
								<span className="text-muted-foreground text-xs">
									{" "}
									· {formatList(tool.languages.map(nameOf), "unit")}
								</span>
							</span>
							{!off && (
								<span
									className="flex gap-1"
									role="group"
									aria-label={t("projectLsps.checks.when", { tool: tool.name })}
								>
									{CHECK_WHEN.map((when) => {
										const on = choice.on ? choice.on.includes(when) : undefined;
										return (
											<button
												key={when}
												type="button"
												disabled={pending}
												aria-pressed={on ?? false}
												title={on === undefined ? t("projectLsps.checks.asRecord") : undefined}
												onClick={() =>
													onSave(
														withTool(settings, tool.id, {
															...choice,
															on: toggledWhen(choice.on, [], when, !on),
														}),
													)
												}
												className={`rounded-full border px-2 py-0.5 text-xs transition-colors ${
													on
														? "border-primary/50 bg-primary/12 text-heading"
														: "border-border text-muted-foreground hover:text-foreground"
												}`}
											>
												{t(`checkWhen.${when}`)}
											</button>
										);
									})}
									{!choice.on && (
										<span className="text-muted-foreground self-center px-1 text-xs">
											{t("projectLsps.checks.asTool")}
										</span>
									)}
									{choice.on && (
										<Button
											size="xs"
											variant="ghost"
											disabled={pending}
											onClick={() => onSave(withTool(settings, tool.id, { ...choice, on: null }))}
										>
											{t("projectLsps.checks.default")}
										</Button>
									)}
								</span>
							)}
							<Switch
								label={t("projectLsps.checks.run", { tool: tool.name })}
								checked={!off}
								disabled={pending}
								onCheckedChange={(next) =>
									onSave(withTool(settings, tool.id, { ...choice, off: next ? null : true }))
								}
							/>
						</li>
					);
				})}
			</ul>
		</Section>
	);
}
