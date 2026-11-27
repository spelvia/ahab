import { Component, type ReactNode } from "react";

import { useContributions } from "@/modules/plugins/runtime/hooks";
import { useTranslation } from "@/shared/i18n";

/** What shows instead of a plugin's content that failed. */
function Failed({ plugin, error }: { plugin: string; error: Error }) {
	const { t } = useTranslation("plugins");
	return (
		<div role="alert" className="text-muted-foreground p-4 text-sm">
			<p className="text-destructive font-medium">{t("runtime.failedHere", { plugin })}</p>
			<p className="mt-1 font-mono text-xs">{error.message}</p>
		</div>
	);
}

/** Keeps a plugin's failure inside its own box. */
class Contained extends Component<{ plugin: string; children: ReactNode }, { error?: Error }> {
	override state: { error?: Error } = {};

	static getDerivedStateFromError(error: Error) {
		return { error };
	}

	override render() {
		if (this.state.error) return <Failed plugin={this.props.plugin} error={this.state.error} />;
		return this.props.children;
	}
}

/** A plugin window's contents, by its plugin and kind. */
export function PluginWindow({
	plugin,
	kind,
	params,
	onClose,
}: {
	plugin: string;
	kind: string;
	params: unknown;
	onClose: () => void;
}) {
	const { t } = useTranslation("plugins");
	const { windows, failures } = useContributions();
	const found = windows.find((window) => window.plugin === plugin && window.kind === kind);
	if (!found)
		return (
			<div className="text-muted-foreground flex flex-col items-start gap-2 p-6 text-sm">
				<p>
					{failures[plugin]
						? t("runtime.failedToLoad", { plugin, error: failures[plugin] })
						: t("runtime.needsPlugin", { plugin })}
				</p>
				<button type="button" onClick={onClose} className="text-foreground underline">
					{t("runtime.closeWindow")}
				</button>
			</div>
		);
	return (
		<Contained plugin={plugin}>
			<div className="h-full overflow-auto">
				<found.Component params={params} />
			</div>
		</Contained>
	);
}

/** A plugin's sidebar view, by its key. */
export function PluginView({ viewKey }: { viewKey: string }) {
	const { views } = useContributions();
	const view = views.find((item) => item.key === viewKey);
	if (!view?.Component) return null;
	return (
		<Contained plugin={view.plugin}>
			<view.Component />
		</Contained>
	);
}

/**
 * Plugins' items (`status.add`) in the status bar, after Ahab's own. Each
 * is kept as tall as the bar, however tall it draws itself.
 */
export function PluginStatusItems({ className }: { className?: string }) {
	const { status } = useContributions();
	if (!status.length) return null;
	return (
		<div className={className ?? "flex h-full items-center"}>
			{status.map((item) => (
				<div
					key={item.key}
					className="flex h-full max-h-full items-center text-xs *:max-h-full *:rounded-none"
				>
					<Contained plugin={item.plugin}>
						<item.Component />
					</Contained>
				</div>
			))}
		</div>
	);
}
