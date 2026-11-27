import { cn } from "cn";
import { ChevronRight, FileDiff } from "lucide-react";
import { useContext, useState } from "react";

import { ChatFileActions, ChatRoot } from "@/modules/sessions/chat/components/chat-root";
import { projectPath } from "@/modules/sessions/chat/components/tool-detail";
import type { TurnChange } from "@/modules/sessions/chat/model";
import { formatNumber, useTranslation } from "@/shared/i18n";
import { FileIcon } from "@/shared/ui/file-icon";
import { toastError } from "@/shared/ui/toast";

/** Rows shown before "Show N more". */
const SHOWN = 6;
/** Up to this many files, the list starts open. */
const OPEN_UP_TO = 3;

const KIND_CLASS = {
	add: "text-emerald-600 dark:text-emerald-400",
	update: "text-amber-600 dark:text-amber-400",
	delete: "text-destructive",
} as const;

/**
 * After a turn: the files it changed, as one summary ("Changed 3 files
 * +68 −4") that opens to a row per file with what happened to it and its
 * lines added and removed. Many files start folded, and a long list shows
 * its first few. A row opens the file's diff, else the file.
 */
export function TurnChanges({
	changes,
	onOpenFile,
}: {
	changes: TurnChange[];
	onOpenFile?: (path: string) => void;
}) {
	const { t } = useTranslation("sessions");
	const [open, setOpen] = useState(changes.length <= OPEN_UP_TO);
	const [all, setAll] = useState(false);
	if (changes.length === 0) return null;
	const added = changes.reduce((sum, change) => sum + change.added, 0);
	const removed = changes.reduce((sum, change) => sum + change.removed, 0);
	const shown = all ? changes : changes.slice(0, SHOWN);
	return (
		<section aria-label={t("turnChanges.section")} className="text-sm">
			<button
				type="button"
				aria-expanded={open}
				onClick={() => setOpen((now) => !now)}
				className="text-muted-foreground hover:text-foreground flex items-center gap-1.5 text-xs"
			>
				<ChevronRight
					className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")}
				/>
				<FileDiff aria-hidden className="size-3.5 shrink-0" />
				<span>{t("conversation.changedFiles", { count: changes.length })}</span>
				<LineCounts added={added} removed={removed} />
			</button>
			{open ? (
				<ul className="border-border mt-1.5 ml-1.5 flex flex-col border-l pl-2">
					{shown.map((change) => (
						<ChangeRow key={change.path} change={change} onOpenFile={onOpenFile} />
					))}
					{changes.length > SHOWN && !all ? (
						<li>
							<button
								type="button"
								onClick={() => setAll(true)}
								className="text-muted-foreground hover:text-foreground px-1.5 py-1 text-xs hover:underline"
							>
								{t("turnChanges.showMore", { count: changes.length - SHOWN })}
							</button>
						</li>
					) : null}
				</ul>
			) : null}
		</section>
	);
}

function ChangeRow({
	change,
	onOpenFile,
}: {
	change: TurnChange;
	onOpenFile?: (path: string) => void;
}) {
	const { t } = useTranslation("sessions");
	const actions = useContext(ChatFileActions);
	const root = useContext(ChatRoot);
	const target = projectPath(change.path, root);
	const slash = change.path.lastIndexOf("/");
	const name = change.path.slice(slash + 1);
	const folder = slash > 0 ? change.path.slice(0, slash) : "";
	const open =
		target && actions.diff && change.kind !== "delete"
			? () => void actions.diff?.(target).catch(toastError)
			: target && onOpenFile && change.exists
				? () => onOpenFile(target)
				: undefined;
	const body = (
		<>
			<FileIcon path={change.path} className="size-3.5 shrink-0" />
			<span className={cn("min-w-0 truncate", change.kind === "delete" && "line-through")}>
				<span className="text-foreground">{name}</span>
				{folder ? <span className="text-muted-foreground ml-1.5 text-xs">{folder}</span> : null}
			</span>
			<span className={cn("ml-auto shrink-0 text-xs", KIND_CLASS[change.kind])}>
				{t(`turnChanges.kind.${change.kind}`)}
			</span>
			<LineCounts added={change.added} removed={change.removed} />
		</>
	);
	return (
		<li>
			{open ? (
				<button
					type="button"
					title={change.path}
					onClick={open}
					className="hover:bg-accent flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left"
				>
					{body}
				</button>
			) : (
				<div title={change.path} className="flex items-center gap-2 px-1.5 py-1">
					{body}
				</div>
			)}
		</li>
	);
}

function LineCounts({ added, removed }: { added: number; removed: number }) {
	const { t } = useTranslation("sessions");
	if (added === 0 && removed === 0) return null;
	return (
		<span
			className="shrink-0 font-mono text-xs tabular-nums"
			aria-label={t("turnChanges.lines", { added, removed })}
		>
			{added > 0 ? (
				<span className="text-emerald-600 dark:text-emerald-400">+{formatNumber(added)}</span>
			) : null}
			{added > 0 && removed > 0 ? " " : null}
			{removed > 0 ? <span className="text-destructive">−{formatNumber(removed)}</span> : null}
		</span>
	);
}
