import { Link } from "@tanstack/react-router";

import { Empty, MachineSection, SectionRows, useMachines } from "@/modules/machines";
import { usePlugins, useRunStates } from "@/modules/plugins/plugin/api";
import { PluginMark, StateDot } from "@/modules/plugins/plugin/components/plugin-parts";
import { formatNumber, useTranslation } from "@/shared/i18n";
import { GlanceItem } from "@/shared/ui/glance";

/**
 * The plugins that run a part on one machine (their daemon entries), and
 * how each is doing there; each opens its page, with its logs.
 */
export function MachinePlugins({ machineId }: { machineId: string }) {
	const { t } = useTranslation("plugins");
	const plugins = usePlugins();
	const online = useMachines().data?.items.find((machine) => machine.id === machineId)?.online;
	const states = useRunStates();
	const here = (plugins.data?.items ?? []).filter(
		(plugin) => plugin.enabled && plugin.entries.includes("daemon"),
	);
	const stateOf = (plugin: string) =>
		states.data?.items.find((item) => item.plugin === plugin && item.machine === machineId);

	return (
		<MachineSection id="plugins" title={t("machine.title")} description={t("machine.description")}>
			{plugins.isPending && <Empty>{t("machine.loading")}</Empty>}
			{plugins.data && here.length === 0 && <Empty>{t("machine.empty")}</Empty>}
			{here.length > 0 && (
				<SectionRows>
					{here.map((plugin) => {
						const state = stateOf(plugin.id);
						return (
							<li
								key={plugin.id}
								className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-0.5 py-2 sm:grid-cols-[12rem_minmax(0,1fr)_auto]"
							>
								<Link
									to="/settings/plugins/$id"
									params={{ id: plugin.id }}
									className="text-heading flex min-w-0 items-center gap-2 font-medium underline-offset-4 hover:underline"
								>
									<PluginMark name={plugin.name} className="size-5 rounded text-[0.625rem]" />
									<span className="truncate">{plugin.name}</span>
								</Link>
								<span className="text-muted-foreground col-span-2 row-start-2 truncate text-xs sm:col-span-1 sm:row-start-auto">
									{state?.message ?? plugin.description}
								</span>
								<span className="text-muted-foreground col-start-2 row-start-1 flex items-center gap-1.5 justify-self-end text-xs sm:col-start-auto sm:row-start-auto">
									{state ? (
										<>
											<StateDot state={state.state} />
											{t(`states.${state.state}`)}
										</>
									) : online ? (
										t("machine.notStarted")
									) : (
										t("machine.offline")
									)}
								</span>
							</li>
						);
					})}
				</SectionRows>
			)}
		</MachineSection>
	);
}

/** How many plugins run a part on a machine, for its overview. */
export function MachinePluginGlance({ machineId }: { machineId: string }) {
	const { t } = useTranslation("plugins");
	const plugins = usePlugins();
	const states = useRunStates();
	const here = (plugins.data?.items ?? []).filter(
		(plugin) => plugin.enabled && plugin.entries.includes("daemon"),
	);
	const running = (states.data?.items ?? []).filter(
		(item) => item.machine === machineId && item.state === "running",
	).length;
	return (
		<GlanceItem
			label={t("glance.label")}
			value={
				plugins.data
					? t("glance.value", { running: formatNumber(running), total: formatNumber(here.length) })
					: t("glance.pending")
			}
			detail={here.map((plugin) => plugin.name).join(", ") || t("glance.none")}
			link={
				<Link
					to="/settings/machines/$id/plugins"
					params={{ id: machineId }}
					className="text-muted-foreground hover:text-foreground underline underline-offset-4"
				>
					{t("glance.link")}
				</Link>
			}
		/>
	);
}
