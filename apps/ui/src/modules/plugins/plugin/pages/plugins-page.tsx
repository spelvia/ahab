import { Link, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useState } from "react";

import { useMachines } from "@/modules/machines";
import { usePluginActions, usePlugins, useRunStates } from "@/modules/plugins/plugin/api";
import { KindPill, PluginMark, StateDot } from "@/modules/plugins/plugin/components/plugin-parts";
import { useContributions } from "@/modules/plugins/runtime";
import type { Plugin, PluginMachineState } from "@/shared/api/generated/ahab.schemas";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";

/** Settings → Plugins: the installed plugins, and adding one. */
export function PluginsPage() {
	const { t } = useTranslation("plugins");
	const plugins = usePlugins();
	const states = useRunStates();
	const [adding, setAdding] = useState(false);
	const items = plugins.data?.items ?? [];

	return (
		<section className="space-y-8">
			<header className="flex flex-wrap items-start justify-between gap-4">
				<div className="space-y-1">
					<h1 className="text-2xl font-semibold">{t("list.title")}</h1>
					<p className="text-muted-foreground max-w-2xl text-sm text-pretty">{t("list.intro")}</p>
				</div>
				{adding ? null : (
					<Button size="sm" onClick={() => setAdding(true)}>
						<Plus />
						{t("list.add")}
					</Button>
				)}
			</header>

			{adding ? <AddPanel onClose={() => setAdding(false)} /> : null}

			{plugins.data && items.length === 0 ? (
				<div className="border-border rounded-lg border border-dashed px-4 py-8 text-center">
					<h2 className="text-base font-bold">{t("list.empty.title")}</h2>
					<p className="text-muted-foreground mx-auto mt-1 max-w-md text-sm">
						<Trans
							t={t}
							i18nKey="list.empty.text"
							components={{ command: <code className="font-mono text-xs" /> }}
						/>
					</p>
				</div>
			) : (
				<ul className="border-border divide-border divide-y border-y">
					{items.map((plugin) => (
						<PluginRow
							key={plugin.id}
							plugin={plugin}
							states={(states.data?.items ?? []).filter((state) => state.plugin === plugin.id)}
						/>
					))}
				</ul>
			)}

			<p className="text-muted-foreground text-sm">
				<Trans
					t={t}
					i18nKey="list.builtIn"
					components={{
						features: (
							<Link
								to="/settings/features"
								className="text-foreground underline underline-offset-4"
							/>
						),
					}}
				/>
			</p>
		</section>
	);
}

function PluginRow({ plugin, states }: { plugin: Plugin; states: PluginMachineState[] }) {
	const { t } = useTranslation("plugins");
	const actions = usePluginActions();
	const machines = useMachines();
	const navigate = useNavigate();
	const { failures } = useContributions();
	const name = (id: string) =>
		machines.data?.items.find((machine) => machine.id === id)?.name ?? id.slice(0, 8);
	const trouble = states.find((state) => state.state !== "running") ?? null;

	return (
		<li className="hover:bg-accent/40 flex items-center gap-3 px-3 py-3 transition-colors duration-150">
			<PluginMark name={plugin.name} />
			<div className="min-w-0 flex-1">
				<div className="flex flex-wrap items-center gap-2">
					<Link
						to="/settings/plugins/$id"
						params={{ id: plugin.id }}
						className="text-heading font-semibold underline-offset-4 hover:underline"
					>
						{plugin.name}
					</Link>
					<span className="text-muted-foreground text-xs">{plugin.version}</span>
					<KindPill plugin={plugin} />
				</div>
				<p className="text-muted-foreground truncate text-sm">{plugin.description}</p>
				{plugin.enabled ? (
					<p className="mt-0.5 flex flex-wrap items-center gap-x-3 text-xs">
						{states.map((state) => (
							<span key={state.machine} className="flex items-center gap-1.5">
								<StateDot state={state.state} />
								<span className="text-muted-foreground">
									{t("list.machineState", {
										machine: name(state.machine),
										state: t(`statesInline.${state.state}`),
									})}
								</span>
							</span>
						))}
						{failures[plugin.id] ? (
							<span className="text-destructive">{t("list.browserFailed")}</span>
						) : null}
						{trouble?.message ? (
							<span className="text-warning-foreground truncate">{trouble.message}</span>
						) : null}
					</p>
				) : null}
			</div>
			<Switch
				label={t(plugin.enabled ? "toggle.disable" : "toggle.enable", { name: plugin.name })}
				checked={plugin.enabled}
				disabled={actions.disable.isPending}
				onCheckedChange={(on) =>
					on
						? // Enabling asks first, on the plugin's page.
							void navigate({
								to: "/settings/plugins/$id",
								params: { id: plugin.id },
								search: { enable: true },
							})
						: actions.disable.mutate({ id: plugin.id })
				}
			/>
		</li>
	);
}

/** Adding a plugin by its folder or Git URL, in place. */
function AddPanel({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation("plugins");
	const { t: tc } = useTranslation();
	const actions = usePluginActions();
	const navigate = useNavigate();
	const [location, setLocation] = useState("");
	return (
		<form
			className="border-border bg-card space-y-3 rounded-lg border p-4"
			onSubmit={(event) => {
				event.preventDefault();
				const value = location.trim();
				const git = /^(https?:\/\/|git@|ssh:\/\/)|\.git$/.test(value);
				actions.install.mutate(
					{ data: { source: git ? "git" : "folder", location: value } },
					{
						onSuccess: (installed) =>
							void navigate({
								to: "/settings/plugins/$id",
								params: { id: installed.plugin.id },
								search: { enable: true, warnings: installed.warnings.join("\n") || undefined },
							}),
					},
				);
			}}
		>
			<div>
				<h2 className="text-base font-bold">{t("add.title")}</h2>
				<p className="text-muted-foreground text-sm">{t("add.intro")}</p>
			</div>
			<input
				// oxlint-disable-next-line jsx-a11y/no-autofocus
				autoFocus
				aria-label={t("add.location")}
				value={location}
				onChange={(event) => setLocation(event.target.value)}
				placeholder={t("add.placeholder")}
				className="border-input bg-background focus-visible:ring-ring h-9 w-full rounded-md border px-3 font-mono text-sm outline-none focus-visible:ring-2"
			/>
			{actions.install.error ? (
				<p role="alert" className="text-destructive text-sm whitespace-pre-wrap">
					{actions.install.error.message}
				</p>
			) : null}
			<div className="flex gap-2">
				<Button type="submit" size="sm" disabled={!location.trim() || actions.install.isPending}>
					{actions.install.isPending ? t("add.adding") : t("add.submit")}
				</Button>
				<Button type="button" size="sm" variant="ghost" onClick={onClose}>
					{tc("actions.cancel")}
				</Button>
			</div>
		</form>
	);
}
