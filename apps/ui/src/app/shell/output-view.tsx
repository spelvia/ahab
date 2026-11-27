import { cn } from "cn";
import { ArrowDownToLine, Ban, Copy } from "lucide-react";
import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { linesSince } from "@/app/shell/panel-state";
import { usePluginLog, usePlugins } from "@/modules/plugins";
import { useCheckLog, useLanguageServerLog, useLanguageServers } from "@/modules/tools";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { useTranslation } from "@/shared/i18n";
import { copyText } from "@/shared/ui/clipboard";
import { toast } from "@/shared/ui/toast";

/** Something the Output tab can show. */
interface Source {
	/** `lsp:<id>`, `checks`, `plugin:<id>`. */
	id: string;
	label: string;
	group: Group;
}

/** The picker's groups, in order; Ahab's own is its name. */
const GROUPS = ["languageServers", "ahab", "plugins"] as const;
type Group = (typeof GROUPS)[number];

const toolbarButton =
	"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 disabled:opacity-40 [&_svg]:size-3.5";

/** A root as the project sees it: relative when inside it. */
function rootLabel(root: string, projectPath: string): string {
	const base = projectPath.replace(/\/+$/, "");
	if (root === base) return "";
	return root.startsWith(`${base}/`) ? root.slice(base.length + 1) : root;
}

/**
 * The Output tab: a log chosen from the project's machine's language
 * servers, the check runs asked for here, and plugins' logs; following new
 * lines while scrolled to the end, with Clear and Copy. The picker and the
 * buttons go in the panel's header (`toolbar`).
 */
export function OutputView({
	project,
	source: chosen,
	onSource,
	toolbar,
}: {
	project: Pick<Project, "id" | "machineId" | "path" | "availability">;
	source: string | undefined;
	onSource: (source: string) => void;
	toolbar: HTMLElement | null;
}) {
	const { t } = useTranslation("shell");
	const online = project.availability === "available";
	const servers = useLanguageServers(project.machineId, online);
	const plugins = usePlugins();
	const sources = useMemo<Source[]>(() => {
		const list: Source[] = [];
		// The project's servers first, then the machine's others.
		const items = (servers.data?.items ?? []).toSorted(
			(a, b) =>
				Number(!a.root.startsWith(project.path)) - Number(!b.root.startsWith(project.path)) ||
				a.name.localeCompare(b.name),
		);
		for (const server of items) {
			const root = rootLabel(server.root, project.path);
			list.push({
				id: `lsp:${server.id}`,
				label: `${server.name}${root ? ` (${root})` : ""}${server.state === "running" ? "" : ` · ${server.state}`}`,
				group: "languageServers",
			});
		}
		list.push({ id: "checks", label: t("output.checkRuns"), group: "ahab" });
		for (const plugin of plugins.data?.items ?? [])
			if (plugin.enabled && plugin.entries.includes("daemon"))
				list.push({ id: `plugin:${plugin.id}`, label: plugin.name, group: "plugins" });
		return list;
	}, [servers.data, plugins.data, project.path, t]);
	const source = sources.find((item) => item.id === chosen) ?? sources[0];

	const lsp = source?.id.startsWith("lsp:") ? source.id.slice(4) : "";
	const pluginId = source?.id.startsWith("plugin:") ? source.id.slice(7) : "";
	const serverLog = useLanguageServerLog(project.machineId, lsp, Boolean(lsp) && online);
	const pluginLog = usePluginLog(pluginId, pluginId && online ? project.machineId : undefined);
	const checkLines = useCheckLog(project.id);
	const query = lsp ? serverLog : pluginId ? pluginLog : undefined;
	const allLines = useMemo(
		() => (lsp ? serverLog.data?.lines : pluginId ? pluginLog.data?.lines : checkLines) ?? [],
		[lsp, pluginId, serverLog.data, pluginLog.data, checkLines],
	);

	// Cleared: the lines after the last one shown then, per source.
	const [marks, setMarks] = useState<Record<string, { line: string; count: number }>>({});
	const lines = source ? linesSince(allLines, marks[source.id]) : allLines;
	const clear = () => {
		if (!source) return;
		const last = allLines.at(-1);
		setMarks((now) => ({
			...now,
			[source.id]: { line: last ?? "", count: allLines.length },
		}));
	};

	// Following: the view stays at the end while it is there.
	const box = useRef<HTMLPreElement>(null);
	const [follow, setFollow] = useState(true);
	const text = lines.join("\n");
	useLayoutEffect(() => {
		const element = box.current;
		if (element && follow && text) element.scrollTop = element.scrollHeight;
	}, [text, follow]);

	const tools = (
		<div className="flex min-w-0 flex-1 items-center justify-end gap-0.5">
			<select
				aria-label={t("output.source")}
				value={source?.id ?? ""}
				onChange={(event) => {
					setFollow(true);
					onSource(event.target.value);
				}}
				className="border-input bg-background h-6 max-w-64 min-w-0 rounded border px-1 text-xs"
			>
				{GROUPS.map((group) => {
					const inGroup = sources.filter((item) => item.group === group);
					return inGroup.length ? (
						<optgroup key={group} label={group === "ahab" ? "Ahab" : t(`output.groups.${group}`)}>
							{inGroup.map((item) => (
								<option key={item.id} value={item.id}>
									{item.label}
								</option>
							))}
						</optgroup>
					) : null;
				})}
			</select>
			<button
				type="button"
				className={cn(toolbarButton, follow && "text-foreground")}
				aria-pressed={follow}
				aria-label={t("output.follow")}
				title={t("output.follow")}
				onClick={() => setFollow(!follow)}
			>
				<ArrowDownToLine />
			</button>
			<button
				type="button"
				className={toolbarButton}
				aria-label={t("output.clear")}
				title={t("output.clearTitle")}
				disabled={!lines.length}
				onClick={clear}
			>
				<Ban />
			</button>
			<button
				type="button"
				className={toolbarButton}
				aria-label={t("output.copy")}
				title={t("output.copy")}
				disabled={!lines.length}
				onClick={() =>
					void copyText(lines.join("\n")).then((copied) =>
						toast(t(copied ? "clipboard.copied" : "clipboard.failed", { ns: "common" })),
					)
				}
			>
				<Copy />
			</button>
		</div>
	);

	return (
		<div className="flex h-full flex-col overflow-hidden">
			{toolbar ? createPortal(tools, toolbar) : <div className="flex px-2 py-1">{tools}</div>}
			<pre
				ref={box}
				aria-label={t("output.label")}
				tabIndex={0}
				onScroll={(event) => {
					const element = event.currentTarget;
					const atEnd = element.scrollHeight - element.scrollTop - element.clientHeight < 8;
					if (atEnd !== follow) setFollow(atEnd);
				}}
				className="min-h-0 flex-1 overflow-auto px-3 py-1.5 font-mono text-xs leading-relaxed whitespace-pre-wrap outline-none"
			>
				{query?.isError ? (
					<span className="text-destructive">{explain(query.error, t("output.unreadable"))}</span>
				) : !online && source?.id !== "checks" ? (
					<span className="text-muted-foreground">{t("output.machineUnavailable")}</span>
				) : query?.isPending && query.fetchStatus !== "idle" ? (
					<span className="text-muted-foreground">{t("state.loading", { ns: "common" })}</span>
				) : text ? (
					text
				) : (
					<span className="text-muted-foreground">
						{source?.id === "checks"
							? t("output.noChecks")
							: allLines.length
								? t("output.cleared")
								: t("output.empty")}
					</span>
				)}
			</pre>
		</div>
	);
}
