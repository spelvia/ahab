import { cn } from "cn";
import { CircleCheck, CircleX, TriangleAlert } from "lucide-react";

import { useProblems } from "@/modules/tools/problems/api";
import { countText } from "@/modules/tools/problems/model";
import { useProblemEvents } from "@/modules/tools/problems/stream";
import { useTranslation } from "@/shared/i18n";
import { mac } from "@/shared/platform";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import { StatusItem } from "@/shared/ui/status-bar";

/**
 * The status bar's `⊗ 2 ⚠ 1`: a project's errors and warnings, live, each
 * only when there are some, and a quiet check when there are none; a
 * click shows or hides the Problems panel.
 */
export function ProblemCounts({
	projectId,
	onToggle,
	onGoToProblem,
	wide = false,
	className,
}: {
	projectId: string;
	onToggle: () => void;
	/** The focused file window's F8 and ⇧F8, offered in the context menu. */
	onGoToProblem?: (direction: 1 | -1) => void;
	/** Room to say the counts in words too (a phone's sheet). */
	wide?: boolean;
	className?: string;
}) {
	const { t } = useTranslation("tools");
	const problems = useProblems(projectId);
	useProblemEvents(projectId);
	const counts = problems.data?.counts ?? { errors: 0, warnings: 0, information: 0, hints: 0 };
	const label = problems.data
		? t("problemCounts.label", { counts: countText(counts) })
		: t("problemCounts.labelLoading");
	const none = !counts.errors && !counts.warnings;
	const item = (
		<StatusItem label={label} onClick={onToggle} className={cn("gap-2 tabular-nums", className)}>
			{none ? <CircleCheck className="size-3.5" /> : null}
			{counts.errors ? (
				<span className="text-foreground flex items-center gap-1">
					<CircleX className="text-destructive size-3.5" />
					{counts.errors}
				</span>
			) : null}
			{counts.warnings ? (
				<span className="text-foreground flex items-center gap-1">
					<TriangleAlert className="text-warning size-3.5" />
					{counts.warnings}
				</span>
			) : null}
			{wide ? <span className="truncate">{countText(counts)}</span> : null}
		</StatusItem>
	);
	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>{item}</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem onSelect={onToggle}>{t("problemCounts.toggle")}</ContextMenuItem>
				<ContextMenuItem disabled={!onGoToProblem} onSelect={() => onGoToProblem?.(1)}>
					{t("problemCounts.next")}
					<span className="text-muted-foreground ml-auto pl-4 text-xs">F8</span>
				</ContextMenuItem>
				<ContextMenuItem disabled={!onGoToProblem} onSelect={() => onGoToProblem?.(-1)}>
					{t("problemCounts.previous")}
					<span className="text-muted-foreground ml-auto pl-4 text-xs">
						{mac ? "⇧F8" : "Shift+F8"}
					</span>
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}

/** "3" beside the Problems tab: the project's errors and warnings, when there are any. */
export function useProblemBadge(projectId: string | undefined): number {
	const problems = useProblems(projectId ?? "", Boolean(projectId));
	const counts = problems.data?.counts;
	return counts ? counts.errors + counts.warnings : 0;
}
