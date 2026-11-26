import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";

import { pluginsQueryKey, usePlugins } from "@/modules/plugins/plugin";
import { createContext } from "@/modules/plugins/runtime/api";
import { shareWithPlugins } from "@/modules/plugins/runtime/components/kit";
import { fail, removePlugin } from "@/modules/plugins/runtime/components/registry";
import { contributedLanguagesOf } from "@/modules/plugins/runtime/model";
import { onEvent } from "@/shared/api/events";
import { t } from "@/shared/i18n";
import { contributeLanguages } from "@/shared/syntax/catalog";

interface Loaded {
	/** Which build: a new digest reloads it. */
	digest: string;
	cleanup?: () => void;
}

/**
 * Loads every enabled plugin's web entry, and reloads or unloads it as
 * plugins change. Renders nothing.
 */
export function PluginLoader() {
	const plugins = usePlugins();
	const client = useQueryClient();
	const loaded = useRef(new Map<string, Loaded>());

	// The server says when plugins change: installed, updated, enabled…
	useEffect(
		() =>
			onEvent("plugins.changed", () => {
				void client.invalidateQueries({ queryKey: pluginsQueryKey() });
			}),
		[client],
	);

	useEffect(() => {
		const items = plugins.data?.items;
		if (!items) return;
		shareWithPlugins();
		const wanted = new Map(
			items
				.filter((plugin) => plugin.enabled && plugin.entries.includes("web"))
				.map((plugin) => [plugin.id, plugin]),
		);
		const unload = (id: string) => {
			try {
				loaded.current.get(id)?.cleanup?.();
			} catch {
				// What it added is removed below all the same.
			}
			loaded.current.delete(id);
			removePlugin(id);
			removeStyles(id);
		};
		for (const id of loaded.current.keys()) {
			if (wanted.get(id)?.digest !== loaded.current.get(id)?.digest) unload(id);
		}
		for (const [id, plugin] of wanted) {
			if (loaded.current.has(id)) continue;
			const entry = (plugin.manifest as { entries?: { web?: unknown } }).entries?.web;
			if (typeof entry !== "string") continue;
			const record: Loaded = { digest: plugin.digest };
			loaded.current.set(id, record);
			const url = `/api/v1/plugins/${id}/files/${entry}?v=${plugin.digest}`;
			addStyles(
				id,
				`/api/v1/plugins/${id}/files/${entry.replace(/\.[cm]?js$/, ".css")}?v=${plugin.digest}`,
			);
			void import(/* @vite-ignore */ url)
				.then((module: { default?: unknown }) => {
					if (loaded.current.get(id) !== record) return;
					if (typeof module.default !== "function")
						throw new Error(t("plugins:runtime.noDefaultExport"));
					const cleanup = (module.default as (context: unknown) => unknown)(createContext(id));
					if (typeof cleanup === "function") record.cleanup = cleanup as () => void;
				})
				.catch((error: unknown) => {
					fail(id, error instanceof Error ? error.message : String(error));
				});
		}
	}, [plugins.data]);

	// Languages enabled plugins declare in their manifests: no
	// web entry needed. Given again when a plugin's build changes.
	const languages = useRef(new Map<string, { digest: string; remove: () => void }>());
	useEffect(() => {
		const items = plugins.data?.items;
		if (!items) return;
		const wanted = new Map(
			items.filter((plugin) => plugin.enabled).map((plugin) => [plugin.id, plugin]),
		);
		for (const [id, held] of languages.current) {
			if (wanted.get(id)?.digest === held.digest) continue;
			held.remove();
			languages.current.delete(id);
		}
		for (const [id, plugin] of wanted) {
			if (languages.current.has(id)) continue;
			const declared = contributedLanguagesOf(id, plugin.digest, plugin.manifest);
			if (!declared.length) continue;
			languages.current.set(id, {
				digest: plugin.digest,
				remove: contributeLanguages(id, declared),
			});
		}
	}, [plugins.data]);
	useEffect(() => {
		const held = languages.current;
		return () => {
			for (const item of held.values()) item.remove();
			held.clear();
		};
	}, []);

	useEffect(() => {
		const current = loaded.current;
		return () => {
			for (const id of current.keys()) {
				removePlugin(id);
				removeStyles(id);
			}
			current.clear();
		};
	}, []);

	return null;
}

/**
 * A web entry's stylesheet, when its build made one (`dist/web.css` beside
 * `dist/web.js`): the plugin's own Tailwind classes, in Ahab's theme.
 */
function addStyles(id: string, href: string) {
	void fetch(href, { method: "HEAD" })
		.then((answer) => {
			if (!answer.ok) return;
			removeStyles(id);
			const link = document.createElement("link");
			link.rel = "stylesheet";
			link.href = href;
			link.dataset.plugin = id;
			document.head.append(link);
		})
		.catch(() => {
			// No stylesheet: the plugin uses Ahab's classes only.
		});
}

function removeStyles(id: string) {
	for (const link of document.head.querySelectorAll<HTMLLinkElement>("link[data-plugin]"))
		if (link.dataset.plugin === id) link.remove();
}
