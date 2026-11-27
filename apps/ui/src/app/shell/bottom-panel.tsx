import { cn } from "cn";
import { Maximize2, Minimize2, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { OutputView } from "@/app/shell/output-view";
import {
	type PanelRequest,
	type PanelState,
	type PanelTab,
	nextPanel,
	panelState,
	savePanel,
	shownHeight,
} from "@/app/shell/panel-state";
import { ProblemsPanel, useProblemBadge } from "@/modules/tools";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { bottomPanel, rememberBottomPanel } from "@/shared/preferences/store";

/**
 * Keeps the bottom panel's arrangement and Output source per project.
 */
export function usePanel(projectId: string | undefined) {
	const [saved, setSaved] = useState(bottomPanel);
	const state = panelState(saved, projectId);
	useEffect(() => {
		const sync = () => setSaved(bottomPanel());
		window.addEventListener("storage", sync);
		window.addEventListener("focus", sync);
		return () => {
			window.removeEventListener("storage", sync);
			window.removeEventListener("focus", sync);
		};
	}, []);
	const change = useCallback(
		(next: (current: ReturnType<typeof bottomPanel>) => ReturnType<typeof bottomPanel>) => {
			const changed = next(bottomPanel());
			rememberBottomPanel(() => changed);
			setSaved(changed);
		},
		[],
	);
	const request = useCallback(
		(asked: PanelRequest) =>
			change((current) =>
				savePanel(current, projectId, nextPanel(panelState(current, projectId), asked)),
			),
		[change, projectId],
	);
	const output = projectId ? saved.output?.[projectId] : undefined;
	const setOutput = useCallback(
		(source: string) => {
			if (!projectId) return;
			change((current) => ({ ...current, output: { ...current.output, [projectId]: source } }));
		},
		[change, projectId],
	);
	return { state, request, output, setOutput };
}

const TABS: { id: PanelTab; label: `bottomPanel.tabs.${PanelTab}` }[] = [
	{ id: "problems", label: "bottomPanel.tabs.problems" },
	{ id: "output", label: "bottomPanel.tabs.output" },
];

const headerButton =
	"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 [&_svg]:size-3.5";

/**
 * The bottom panel, as VS Code's: under the windows, between the sidebars;
 * Problems and Output tabs, the tab's own tools in the header, maximise and
 * close; resized by dragging its top edge. Phones show it full-screen.
 */
export function BottomPanel({
	state,
	onRequest,
	project,
	output,
	onOutput,
	onOpenFile,
	phone,
	hints,
}: {
	state: PanelState;
	onRequest: (request: PanelRequest) => void;
	project: Project;
	output: string | undefined;
	onOutput: (source: string) => void;
	onOpenFile: (projectId: string, path: string, line?: number) => void;
	phone: boolean;
	/** The keys of Toggle panel, and of each tab. */
	hints: { toggle?: string; problems?: string; output?: string };
}) {
	const { t } = useTranslation("shell");
	const [toolbar, setToolbar] = useState<HTMLDivElement | null>(null);
	const [dragging, setDragging] = useState<number | null>(null);
	const start = useRef<{ y: number; height: number; available: number } | null>(null);
	const section = useRef<HTMLElement>(null);
	const badge = useProblemBadge(project.id);
	const maximised = phone || state.maximised;
	const height = dragging ?? state.height;

	return (
		<section
			ref={section}
			aria-label={t("bottomPanel.label")}
			style={maximised ? undefined : { height }}
			className={cn(
				"bg-background border-border relative flex min-h-0 shrink-0 flex-col border-t",
				maximised ? "flex-1" : "max-h-[calc(100%-7.5rem)]",
			)}
		>
			{maximised ? null : (
				<div
					role="separator"
					aria-orientation="horizontal"
					aria-label={t("bottomPanel.resize")}
					onPointerDown={(event) => {
						event.currentTarget.setPointerCapture(event.pointerId);
						const available = section.current?.parentElement?.clientHeight ?? 0;
						start.current = {
							y: event.clientY,
							height: section.current?.clientHeight ?? height,
							available,
						};
					}}
					onPointerMove={(event) => {
						if (!start.current) return;
						const next = start.current.height - (event.clientY - start.current.y);
						setDragging(shownHeight(next, start.current.available));
					}}
					onPointerUp={() => {
						start.current = null;
						if (dragging !== null) onRequest({ kind: "resize", height: dragging });
						setDragging(null);
					}}
					onDoubleClick={() => onRequest({ kind: "maximise", on: true })}
					className={cn(
						"hover:bg-ring/50 absolute inset-x-0 -top-0.5 z-10 h-1 cursor-row-resize transition-colors",
						dragging !== null && "bg-ring/60",
					)}
				/>
			)}
			<div className="flex min-h-8 shrink-0 flex-wrap items-center gap-x-2 gap-y-1 px-2 py-0.5">
				<div
					role="tablist"
					aria-label={t("bottomPanel.label")}
					className="flex items-center gap-0.5"
				>
					{TABS.map((tab) => {
						const key = tab.id === "problems" ? hints.problems : hints.output;
						const label = t(tab.label);
						return (
							<button
								key={tab.id}
								type="button"
								role="tab"
								aria-selected={state.tab === tab.id}
								onClick={() => onRequest({ kind: "show", tab: tab.id })}
								title={key ? t("withKey", { ns: "common", label, key }) : label}
								className={cn(
									"focus-visible:ring-ring flex h-6 items-center gap-1.5 rounded px-2 text-xs outline-none focus-visible:ring-2",
									state.tab === tab.id
										? "bg-accent text-foreground"
										: "text-muted-foreground hover:text-foreground",
								)}
							>
								{label}
								{tab.id === "problems" && badge ? (
									<span className="bg-muted-foreground/20 rounded-full px-1.5 text-[0.625rem]">
										{badge}
									</span>
								) : null}
							</button>
						);
					})}
				</div>
				<div ref={setToolbar} className="flex min-w-0 flex-1 items-center" />
				<div className="ml-auto flex items-center gap-0.5">
					{phone ? null : (
						<button
							type="button"
							className={headerButton}
							aria-label={state.maximised ? t("bottomPanel.restore") : t("bottomPanel.maximise")}
							title={state.maximised ? t("bottomPanel.restore") : t("bottomPanel.maximise")}
							onClick={() => onRequest({ kind: "maximise", on: !state.maximised })}
						>
							{state.maximised ? <Minimize2 /> : <Maximize2 />}
						</button>
					)}
					<button
						type="button"
						className={headerButton}
						aria-label={t("bottomPanel.close")}
						title={
							hints.toggle
								? t("withKey", { ns: "common", label: t("bottomPanel.close"), key: hints.toggle })
								: t("bottomPanel.close")
						}
						onClick={() => onRequest({ kind: "close" })}
					>
						<X />
					</button>
				</div>
			</div>
			<div className="min-h-0 flex-1">
				{state.tab === "problems" ? (
					<ProblemsPanel project={project} toolbar={toolbar} onOpenFile={onOpenFile} />
				) : (
					<OutputView
						key={project.id}
						project={project}
						source={output}
						onSource={onOutput}
						toolbar={toolbar}
					/>
				)}
			</div>
		</section>
	);
}
