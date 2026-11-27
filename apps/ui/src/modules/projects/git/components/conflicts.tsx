import { cn } from "cn";
import { Check } from "lucide-react";

import { LETTER_TONE } from "@/modules/projects/git/components/changes";
import type { GitChange, GitStatus } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { FileIcon } from "@/shared/ui/file-icon";
import { IconButton } from "@/shared/ui/icon-button";

/** An unfinished merge, rebase, cherry-pick or revert: continue or abort. */
export function Operation({
	status,
	onContinue,
	onAbort,
}: {
	status: GitStatus;
	onContinue: () => void;
	onAbort: () => void;
}) {
	const { t } = useTranslation("projects");
	const count = status.conflicts.length;
	const operation =
		status.operation === "merge"
			? count
				? t("conflicts.operation.merge", { count })
				: t("conflicts.operation.mergeResolved")
			: status.operation === "rebase"
				? count
					? t("conflicts.operation.rebase", { count })
					: t("conflicts.operation.rebaseResolved")
				: status.operation === "cherryPick"
					? count
						? t("conflicts.operation.cherryPick", { count })
						: t("conflicts.operation.cherryPickResolved")
					: count
						? t("conflicts.operation.revert", { count })
						: t("conflicts.operation.revertResolved");
	return (
		<div className="border-warning/40 bg-warning/8 mx-3 rounded-md border p-2.5 text-xs">
			<p className="text-heading mb-2 font-medium">{operation}</p>
			<div className="flex gap-2">
				<Button size="xs" disabled={status.conflicts.length > 0} onClick={onContinue}>
					{t("conflicts.continue")}
				</Button>
				<Button size="xs" variant="ghost" onClick={onAbort}>
					{t("conflicts.abort")}
				</Button>
			</div>
		</div>
	);
}

/** Conflicted files: open one, then mark it resolved. */
export function Conflicts({
	changes,
	readOnly,
	onOpen,
	onResolve,
}: {
	changes: GitChange[];
	readOnly: boolean;
	onOpen: (change: GitChange) => void;
	onResolve: (change: GitChange) => void;
}) {
	const { t } = useTranslation("projects");
	if (changes.length === 0) return null;
	return (
		<section aria-label={t("conflicts.title")}>
			<p className="text-destructive px-3 py-1 text-[0.6875rem] font-semibold tracking-wider uppercase">
				{t("conflicts.title")}
				<span className="bg-destructive/12 ml-1.5 rounded-full px-1.5 text-[0.625rem] tracking-normal">
					{changes.length}
				</span>
			</p>
			<ul>
				{changes.map((change) => (
					<li
						key={change.path}
						className="group/row hover:bg-accent/60 flex h-7 items-center gap-1 pr-1.5 pl-5"
					>
						<button
							type="button"
							onClick={() => onOpen(change)}
							title={change.path}
							className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left text-[0.8125rem]"
						>
							<FileIcon path={change.path} className="size-3.5" />
							<span className="truncate">{change.path}</span>
						</button>
						{readOnly ? null : (
							<IconButton label={t("conflicts.markResolved")} onClick={() => onResolve(change)}>
								<Check />
							</IconButton>
						)}
						<span
							className={cn("w-3 text-center font-mono text-xs font-semibold", LETTER_TONE["!"])}
						>
							!
						</span>
					</li>
				))}
			</ul>
		</section>
	);
}
