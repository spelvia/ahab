import { Link, useNavigate } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { ChevronRight, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useMachines } from "@/modules/machines";
import {
	useLogs,
	usePlugin,
	usePluginActions,
	usePlugins,
	usePluginText,
	useRunStates,
} from "@/modules/plugins/plugin/api";
import {
	KindPill,
	PluginMark,
	Row,
	Rows,
	Section,
	StateDot,
} from "@/modules/plugins/plugin/components/plugin-parts";
import { useContributions } from "@/modules/plugins/runtime";
import { SettingsForm } from "@/modules/plugins/setting";
import type { Plugin, PluginMachineState } from "@/shared/api/generated/ahab.schemas";
import { formatDate, formatNumber, Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Markdown } from "@/shared/ui/markdown";
import { Switch } from "@/shared/ui/switch";

const PARTS = ["web", "daemon", "server"] as const;

/** What a part of a plugin does, or its name when Ahab does not know it. */
function partText(t: TFunction<"plugins">, entry: string): string {
	return (PARTS as readonly string[]).includes(entry)
		? t(`parts.${entry as (typeof PARTS)[number]}`)
		: entry;
}

function when(t: TFunction<"plugins">, value: string | null | undefined): string {
	return value
		? formatDate(value, {
				year: "numeric",
				month: "numeric",
				day: "numeric",
				hour: "numeric",
				minute: "numeric",
				second: "numeric",
			})
		: t("details.never");
}

/** One plugin: what it is and does, its settings, where it runs, its history. */
export function PluginPage({
	id,
	enable,
	warnings,
}: {
	id: string;
	/** Arrived to enable it: ask at once. */
	enable?: boolean;
	/** What its manifest should fix, from installing it. */
	warnings?: string;
}) {
	const { t } = useTranslation("plugins");
	const { t: tc } = useTranslation();
	const plugin = usePlugin(id);
	const navigate = useNavigate();
	const actions = usePluginActions();
	const [asking, setAsking] = useState(Boolean(enable));
	const [removing, setRemoving] = useState(false);

	if (plugin.isError)
		return (
			<section className="space-y-2">
				<Breadcrumb name={t("page.unknownName")} />
				<p className="text-muted-foreground text-sm">{t("page.missing")}</p>
			</section>
		);
	if (!plugin.data) return <Breadcrumb name={t("page.unknownName")} />;
	const data = plugin.data;

	return (
		<section className="space-y-8">
			<Breadcrumb name={data.name} />
			<header className="flex flex-wrap items-start gap-4">
				<PluginMark name={data.name} className="size-12 text-lg" />
				<div className="min-w-0 flex-1 space-y-1">
					<div className="flex flex-wrap items-center gap-2">
						<h1 className="text-2xl font-semibold">{data.name}</h1>
						<span className="text-muted-foreground text-sm">{data.version}</span>
						<KindPill plugin={data} />
					</div>
					{data.description ? (
						<p className="text-muted-foreground max-w-2xl text-sm text-pretty">
							{data.description}
						</p>
					) : null}
				</div>
				<div className="flex items-center gap-3">
					<label className="text-muted-foreground flex items-center gap-2 text-sm">
						{data.enabled ? t("page.enabled") : t("page.disabled")}
						<Switch
							label={t(data.enabled ? "toggle.disable" : "toggle.enable", { name: data.name })}
							checked={data.enabled}
							onCheckedChange={(on) => (on ? setAsking(true) : actions.disable.mutate({ id }))}
						/>
					</label>
					<DropdownMenu>
						<DropdownMenuTrigger
							aria-label={t("page.more")}
							className="text-muted-foreground hover:text-foreground hover:bg-accent grid size-8 place-items-center rounded-md"
						>
							<MoreHorizontal className="size-4" />
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end">
							<DropdownMenuItem onSelect={() => actions.update.mutate({ id })}>
								<RefreshCw />
								{data.source === "git"
									? t("page.updateFromRepository")
									: t("page.updateFromFolder")}
							</DropdownMenuItem>
							<DropdownMenuItem disabled={data.enabled} onSelect={() => setRemoving(true)}>
								<Trash2 />
								{t("page.remove")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</div>
			</header>

			{warnings ? (
				<Band tone="warn">
					<p className="font-medium">{t("page.manifestWarnings")}</p>
					<ul className="mt-1 list-disc pl-5">
						{warnings.split("\n").map((warning) => (
							<li key={warning}>{warning}</li>
						))}
					</ul>
				</Band>
			) : null}
			{actions.update.isPending ? <Band>{t("page.updating")}</Band> : null}
			{actions.update.error || actions.disable.error ? (
				<Band tone="danger">{(actions.update.error ?? actions.disable.error)?.message}</Band>
			) : null}

			{asking && !data.enabled ? (
				<EnablePanel plugin={data} onDone={() => setAsking(false)} />
			) : null}

			{removing ? (
				<Band tone="danger">
					<div className="flex flex-wrap items-center gap-3">
						<p className="flex-1">{t("page.removeConfirm", { name: data.name })}</p>
						<Button
							size="sm"
							variant="destructive"
							disabled={actions.remove.isPending}
							onClick={() =>
								actions.remove.mutate(
									{ id },
									{ onSuccess: () => void navigate({ to: "/settings/plugins" }) },
								)
							}
						>
							{tc("actions.remove")}
						</Button>
						<Button size="sm" variant="ghost" onClick={() => setRemoving(false)}>
							{tc("actions.cancel")}
						</Button>
					</div>
				</Band>
			) : null}

			<About plugin={data} />
			<PluginSettings plugin={data} />
			<WhatItDoes plugin={data} />
			{data.entries.includes("daemon") ? <Machines plugin={data} /> : null}
			<Changelog plugin={data} />
			<Section title={t("details.title")}>
				<Rows>
					<Row label={t("details.id")}>
						<span className="font-mono text-xs">{data.id}</span>
					</Row>
					<Row label={t("details.installedFrom")}>
						<span className="flex items-center gap-1">
							<span className="truncate font-mono text-xs" title={data.location}>
								{data.location}
							</span>
							<CopyButton text={data.location} label={t("details.location")} />
						</span>
					</Row>
					<Row label={t("details.build")} hint={t("details.buildHint")}>
						<span className="font-mono text-xs" title={data.digest}>
							{data.digest.slice(0, 16)}
						</span>
					</Row>
					<Row label={t("details.installed")}>{when(t, data.createdAt)}</Row>
					<Row label={t("details.updated")}>{when(t, data.updatedAt)}</Row>
				</Rows>
			</Section>
		</section>
	);
}

function Breadcrumb({ name }: { name: string }) {
	const { t } = useTranslation("plugins");
	return (
		<nav
			aria-label={t("page.breadcrumb")}
			className="text-muted-foreground flex items-center gap-1 text-sm"
		>
			<Link
				to="/settings/plugins"
				className="hover:text-foreground underline-offset-4 hover:underline"
			>
				{t("page.breadcrumbPlugins")}
			</Link>
			<ChevronRight className="size-3.5" />
			<span className="text-foreground">{name}</span>
		</nav>
	);
}

function Band({ tone, children }: { tone?: "warn" | "danger"; children: ReactNode }) {
	return (
		<div
			className={
				tone === "danger"
					? "border-destructive/30 bg-destructive/6 rounded-lg border px-4 py-3 text-sm"
					: tone === "warn"
						? "border-warning/50 bg-warning/12 rounded-lg border px-4 py-3 text-sm"
						: "border-border bg-card rounded-lg border px-4 py-3 text-sm"
			}
		>
			{children}
		</div>
	);
}

/** What enabling lets it do, the machines for a placed plugin, then Enable. */
function EnablePanel({ plugin, onDone }: { plugin: Plugin; onDone: () => void }) {
	const { t } = useTranslation("plugins");
	const actions = usePluginActions();
	const machines = useMachines();
	const [picked, setPicked] = useState<string[]>(plugin.machines);
	const chosen = plugin.placement === "chosen";
	return (
		<div className="border-warning/50 bg-warning/12 space-y-3 rounded-lg border px-4 py-4">
			<div>
				<h2 className="text-base font-bold">{t("enable.title", { name: plugin.name })}</h2>
				<p className="text-sm">{t("enable.intro")}</p>
			</div>
			<Capabilities plugin={plugin} />
			{chosen ? (
				<fieldset className="space-y-1 text-sm">
					<legend className="font-medium">{t("enable.machines")}</legend>
					{(machines.data?.items ?? []).map((machine) => (
						<label key={machine.id} className="flex items-center gap-2">
							<input
								type="checkbox"
								checked={picked.includes(machine.id)}
								onChange={(event) =>
									setPicked((now) =>
										event.target.checked
											? [...now, machine.id]
											: now.filter((item) => item !== machine.id),
									)
								}
							/>
							{machine.name}
						</label>
					))}
				</fieldset>
			) : null}
			{actions.enable.error ? (
				<p role="alert" className="text-destructive text-sm">
					{actions.enable.error.message}
				</p>
			) : null}
			<div className="flex gap-2">
				<Button
					size="sm"
					disabled={actions.enable.isPending || (chosen && picked.length === 0)}
					onClick={() =>
						actions.enable.mutate(
							{ id: plugin.id, data: chosen ? { machines: picked } : {} },
							{ onSuccess: onDone },
						)
					}
				>
					{t("enable.submit")}
				</Button>
				<Button size="sm" variant="ghost" onClick={onDone}>
					{t("enable.notNow")}
				</Button>
			</div>
		</div>
	);
}

function Capabilities({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	return (
		<ul className="list-disc space-y-0.5 pl-5 text-sm">
			{plugin.entries.map((entry) => (
				<li key={entry}>{partText(t, entry)}</li>
			))}
			{plugin.hooks.map((hook) => (
				<li key={hook}>
					<Trans
						t={t}
						i18nKey="capabilities.hook"
						values={{ hook }}
						components={{ hook: <code className="font-mono text-xs" /> }}
					/>
				</li>
			))}
			{plugin.secrets.length ? (
				<li>{t("capabilities.secrets", { secrets: plugin.secrets.join(", ") })}</li>
			) : null}
			{Object.keys(plugin.dependsOn).length ? (
				<li>{t("capabilities.uses", { plugins: Object.keys(plugin.dependsOn).join(", ") })}</li>
			) : null}
		</ul>
	);
}

/** Its page: OVERVIEW.md, else README.md, else its description. */
function About({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const overview = usePluginText(plugin.id, "OVERVIEW.md", plugin.digest);
	const readme = usePluginText(
		plugin.id,
		"README.md",
		overview.data === null ? plugin.digest : undefined,
	);
	const text = overview.data ?? readme.data;
	if (!text) return null;
	return (
		<Section title={t("about.title")}>
			<div className="border-border border-y py-4">
				<Markdown text={text.replace(/^#\s.*\n+/, "")} className="max-w-3xl text-sm" />
			</div>
		</Section>
	);
}

function PluginSettings({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const { settings: pages } = useContributions();
	const page = pages.find((item) => item.plugin === plugin.id);
	const schema = (plugin.manifest as { settings?: unknown }).settings;
	if (!page && !schema) return null;
	return (
		<Section title={t("settings.title")}>
			{page ? (
				<div className="border-border border-y py-4">
					<page.Component />
				</div>
			) : null}
			{schema && page ? (
				<h3 className="text-muted-foreground pt-2 text-xs font-semibold tracking-wide uppercase">
					{t("settings.other")}
				</h3>
			) : null}
			{schema ? <SettingsForm plugin={plugin.id} simpleOnly={Boolean(page)} /> : null}
		</Section>
	);
}

function WhatItDoes({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const machines = useMachines();
	const plugins = usePlugins();
	const actions = usePluginActions();
	const list = plugins.data?.items ?? [];
	const at = list.findIndex((item) => item.id === plugin.id);
	const swap = (other: Plugin | undefined) => {
		if (!other) return;
		actions.change.mutate({ id: plugin.id, data: { position: other.position } });
		actions.change.mutate({ id: other.id, data: { position: plugin.position } });
	};
	return (
		<Section title={t("whatItDoes.title")}>
			<Rows>
				<Row label={t("whatItDoes.parts")}>
					{plugin.entries.map((entry) => partText(t, entry)).join("; ")}
				</Row>
				<Row label={t("whatItDoes.runsOn")}>
					{plugin.placement === "all"
						? t("whatItDoes.everyMachine")
						: plugin.machines
								.map((id) => machines.data?.items.find((machine) => machine.id === id)?.name ?? id)
								.join(", ") || t("whatItDoes.noMachine")}
				</Row>
				{plugin.hooks.length ? (
					<Row label={t("whatItDoes.hooks")} hint={t("whatItDoes.hooksHint")}>
						<div className="flex flex-wrap items-center gap-2">
							{plugin.hooks.map((hook) => (
								<code key={hook} className="bg-muted rounded px-1.5 py-0.5 font-mono text-xs">
									{hook}
								</code>
							))}
							<span className="text-muted-foreground text-xs">
								{t("whatItDoes.position", {
									position: formatNumber(at + 1),
									total: formatNumber(list.length),
								})}
							</span>
							<Button
								size="sm"
								variant="ghost"
								disabled={at <= 0}
								onClick={() => swap(list[at - 1])}
							>
								{t("whatItDoes.earlier")}
							</Button>
							<Button
								size="sm"
								variant="ghost"
								disabled={at >= list.length - 1}
								onClick={() => swap(list[at + 1])}
							>
								{t("whatItDoes.later")}
							</Button>
						</div>
					</Row>
				) : null}
				{plugin.secrets.length ? (
					<Row label={t("whatItDoes.secrets")}>{plugin.secrets.join(", ")}</Row>
				) : null}
				{Object.keys(plugin.dependsOn).length ? (
					<Row label={t("whatItDoes.uses")}>
						{Object.entries(plugin.dependsOn)
							.map(([name, range]) => `${name} ${range}`)
							.join(", ")}
					</Row>
				) : null}
			</Rows>
		</Section>
	);
}

/** Where it runs and how it is doing; logs open beneath a machine. */
function Machines({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const states = useRunStates();
	const machines = useMachines();
	const mine = (states.data?.items ?? []).filter((state) => state.plugin === plugin.id);
	return (
		<Section title={t("machines.title")} note={plugin.enabled ? undefined : t("machines.disabled")}>
			{plugin.enabled && mine.length === 0 ? (
				<p className="text-muted-foreground text-sm">{t("machines.noneOnline")}</p>
			) : null}
			{mine.length ? (
				<ul className="border-border divide-border divide-y border-y text-sm">
					{mine.map((state) => (
						<MachineState
							key={state.machine}
							plugin={plugin.id}
							state={state}
							name={
								machines.data?.items.find((machine) => machine.id === state.machine)?.name ??
								state.machine
							}
						/>
					))}
				</ul>
			) : null}
		</Section>
	);
}

function MachineState({
	plugin,
	state,
	name,
}: {
	plugin: string;
	state: PluginMachineState;
	name: string;
}) {
	const { t } = useTranslation("plugins");
	const [open, setOpen] = useState(false);
	const logs = useLogs(plugin, open ? state.machine : undefined);
	return (
		<li className="py-2.5">
			<div className="flex flex-wrap items-center gap-2">
				<StateDot state={state.state} />
				<span className="font-medium">{name}</span>
				<span className="text-muted-foreground">
					{t("machines.since", {
						state: t(`states.${state.state}`),
						time: formatDate(state.since, {
							hour: "numeric",
							minute: "numeric",
							second: "numeric",
						}),
					})}
				</span>
				<Button
					size="sm"
					variant="ghost"
					className="ml-auto"
					onClick={() => setOpen((now) => !now)}
				>
					{open ? t("machines.hideLogs") : t("machines.showLogs")}
				</Button>
			</div>
			{state.message ? (
				<p className="text-warning-foreground mt-1 text-xs">{state.message}</p>
			) : null}
			{open ? (
				<pre className="bg-muted mt-2 max-h-80 overflow-auto rounded-md p-3 font-mono text-xs whitespace-pre-wrap">
					{logs.error ? logs.error.message : logs.data?.lines.join("\n") || t("machines.noLogs")}
				</pre>
			) : null}
		</li>
	);
}

function Changelog({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const changelog = usePluginText(plugin.id, "CHANGELOG.md", plugin.digest);
	return (
		<Section title={t("changelog.title")}>
			{changelog.data ? (
				<div className="border-border border-y py-4">
					<Markdown text={changelog.data.replace(/^#\s.*\n+/, "")} className="max-w-3xl text-sm" />
				</div>
			) : (
				<p className="text-muted-foreground text-sm">
					<Trans
						t={t}
						i18nKey="changelog.missing"
						values={{ name: plugin.name }}
						components={{ file: <code className="font-mono text-xs" /> }}
					/>
				</p>
			)}
		</Section>
	);
}
