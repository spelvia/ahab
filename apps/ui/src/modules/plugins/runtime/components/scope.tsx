/**
 * Where a plugin's stylesheet applies. `ahab-plugin build` scopes a web
 * entry's Tailwind classes to `[data-ahab-plugin="<id>"]`; everything a
 * plugin renders is wrapped in that mark here, so its classes reach its own
 * content and nothing else of Ahab's or another plugin's.
 */
import { type ComponentType, createContext, type ReactNode, useContext } from "react";

const Scope = createContext<string | undefined>(undefined);

/** The plugin whose content this is, inside a plugin's views and windows. */
export function usePluginScope(): string | undefined {
	return useContext(Scope);
}

/** Marks what a plugin renders, without a box of its own (`display: contents`). */
function PluginScope({ plugin, children }: { plugin: string; children: ReactNode }) {
	return (
		<Scope value={plugin}>
			<div data-ahab-plugin={plugin} className="contents">
				{children}
			</div>
		</Scope>
	);
}

/** A plugin's icon, marked the same way, for where only inline content fits. */
export function scopedIcon(plugin: string, icon: ReactNode): ReactNode {
	if (icon === undefined || icon === null) return icon;
	return (
		<span data-ahab-plugin={plugin} className="contents">
			{icon}
		</span>
	);
}

/** A plugin's component, rendered inside its scope. */
export function scoped<P extends object>(plugin: string, Component: ComponentType<P>) {
	function Scoped(props: P) {
		return (
			<PluginScope plugin={plugin}>
				<Component {...props} />
			</PluginScope>
		);
	}
	Scoped.displayName = `Plugin(${plugin}:${Component.displayName ?? Component.name ?? "component"})`;
	return Scoped;
}
