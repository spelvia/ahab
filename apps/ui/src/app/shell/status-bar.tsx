import { cn } from "cn";
import { CircleAlert } from "lucide-react";

import { MachineStatusDot, MachineStatusItem } from "@/modules/machines";
import { PluginStatusItems, useContributions } from "@/modules/plugins";
import { BranchButton, FileStatusItems, useFileStatus } from "@/modules/projects";
import { ProblemCounts, useProblemBadge } from "@/modules/tools";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { VimStatusItem } from "@/shared/editor/vim-status-item";
import { useTranslation } from "@/shared/i18n";
import { Sheet, SheetContent } from "@/shared/ui/sheet";
import { statusItemClass } from "@/shared/ui/status-bar";

/** What the status bar and the phone's status sheet show, and what their items do. */
export type StatusProps = {
	project: Project;
	repository?: string;
	/** Whether the Git feature is on. */
	git: boolean;
	onBranch: () => void;
	onProblems: () => void;
	/** The focused file window's status key, if a file window is focused. */
	fileKey: string | undefined;
};

/** The project's branch, while Git is on and the project can be reached. */
function Branch({
	project,
	repository,
	git,
	onBranch,
	className,
}: Pick<StatusProps, "project" | "repository" | "git" | "onBranch"> & { className: string }) {
	if (!git || project.availability !== "available") return null;
	return (
		<BranchButton
			project={project}
			repository={repository}
			onOpen={onBranch}
			className={className}
		/>
	);
}

/**
 * The status bar along the bottom of the workbench, on desktops only, in
 * three groups set apart by space: where the work is (the machine and its
 * connection, the Git branch), the problem counts (which show or hide the
 * Problems panel) and plugins' items on the left; the focused file
 * window's state on the right (Vim's mode, the cursor, indentation,
 * encoding, line endings, language, language tools, Read-only or Editing).
 * A group with nothing to say takes no room. The inbox is the top bar's
 * bell; phones have no status bar but the status sheet instead.
 */
export function StatusBar({
	project,
	repository,
	git,
	onBranch,
	onProblems,
	fileKey,
}: StatusProps) {
	const { t } = useTranslation("shell");
	const file = useFileStatus(fileKey).status;
	return (
		<footer
			aria-label={t("statusBar.label")}
			className="bg-muted/40 border-border text-muted-foreground flex h-6 shrink-0 items-stretch justify-between gap-6 overflow-hidden border-t px-1 text-xs"
		>
			<div className="flex min-w-0 items-stretch gap-3">
				<div role="group" aria-label={t("statusBar.where")} className="flex min-w-0 items-stretch">
					<MachineStatusItem machineId={project.machineId} name={project.machineName} />
					<Branch
						project={project}
						repository={repository}
						git={git}
						onBranch={onBranch}
						className={cn(statusItemClass, "h-full max-w-64 shrink rounded-none px-2 text-xs")}
					/>
				</div>
				<ProblemCounts
					projectId={project.id}
					onToggle={onProblems}
					onGoToProblem={file?.goToProblem}
				/>
				<PluginStatusItems className="flex min-w-0 items-stretch" />
			</div>
			{fileKey ? (
				<div
					role="group"
					aria-label={t("statusBar.file")}
					className="flex min-w-0 shrink-0 items-stretch justify-end"
				>
					<VimStatusItem />
					<FileStatusItems statusKey={fileKey} />
				</div>
			) : null}
		</footer>
	);
}

/**
 * A phone's way to the status bar's items, in its top bar: the machine's
 * connection as a dot and the problem count when there are problems; a
 * tap opens the status sheet.
 */
export function PhoneStatusButton({ project, onOpen }: { project: Project; onOpen: () => void }) {
	const { t } = useTranslation("shell");
	const problems = useProblemBadge(project.id);
	return (
		<button
			type="button"
			onClick={onOpen}
			title={t("statusBar.status")}
			className="text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-ring flex h-11 min-w-11 shrink-0 items-center justify-center gap-1.5 rounded-md px-2 outline-none focus-visible:ring-2"
		>
			<span className="sr-only">{t("statusBar.status")}</span>
			<MachineStatusDot machineId={project.machineId} />
			{problems > 0 ? (
				<span className="text-foreground flex items-center gap-0.5 text-xs font-medium tabular-nums">
					<CircleAlert className="size-3.5" aria-hidden />
					{problems}
					<span className="sr-only">{t("statusBar.problems", { count: problems })}</span>
				</span>
			) : null}
		</button>
	);
}

/** A heading over a group of the status sheet. */
function SheetGroup({ label, children }: { label: string; children: React.ReactNode }) {
	return (
		<section aria-label={label} className="border-border border-t px-3 pt-3 pb-2">
			<h3 className="text-muted-foreground px-2 pb-1 text-xs font-medium">{label}</h3>
			{children}
		</section>
	);
}

/** Every item the desktop status bar shows, as rows of 44px or more. */
const row = "h-12 w-full rounded-lg px-3 text-sm";

/**
 * The status bar's items on a phone, in a sheet: the machine and its
 * connection, the branch and the problems as full-width rows, then
 * plugins' items, then the focused file window's.
 */
export function PhoneStatusSheet({
	open,
	onOpenChange,
	project,
	repository,
	git,
	onBranch,
	onProblems,
	fileKey,
}: StatusProps & { open: boolean; onOpenChange: (open: boolean) => void }) {
	const { t } = useTranslation("shell");
	const { status: plugins } = useContributions();
	const file = useFileStatus(fileKey).status;
	const close = (then: () => void) => () => {
		onOpenChange(false);
		then();
	};
	return (
		<Sheet open={open} onOpenChange={onOpenChange}>
			<SheetContent title={t("statusBar.sheet")}>
				<div className="min-h-0 flex-1 overflow-y-auto">
					<div className="flex flex-col gap-0.5 px-3 pb-3">
						<MachineStatusItem
							machineId={project.machineId}
							name={project.machineName}
							wide
							className={row}
						/>
						<Branch
							project={project}
							repository={repository}
							git={git}
							onBranch={close(onBranch)}
							className={cn(statusItemClass, row, "gap-1.5")}
						/>
						<ProblemCounts
							projectId={project.id}
							onToggle={close(onProblems)}
							onGoToProblem={file?.goToProblem}
							wide
							className={row}
						/>
					</div>
					{plugins.length ? (
						<SheetGroup label={t("statusBar.plugins")}>
							<PluginStatusItems className="flex flex-wrap gap-1 text-sm *:h-11 *:rounded-lg" />
						</SheetGroup>
					) : null}
					{file ? (
						<SheetGroup label={t("statusBar.file")}>
							<div className="flex flex-wrap gap-1 text-sm *:h-11 *:rounded-lg *:px-3">
								<VimStatusItem />
								<FileStatusItems statusKey={fileKey} />
							</div>
						</SheetGroup>
					) : null}
				</div>
			</SheetContent>
		</Sheet>
	);
}
