import { cn } from "cn";
import { ChevronRight, MessageSquarePlus } from "lucide-react";
import { useState } from "react";

import { useTurnDiagnostics } from "@/modules/tools/problems/api";
import { ProblemRow } from "@/modules/tools/problems/components/problem-row";
import { total, turnProblemsText, turnText } from "@/modules/tools/problems/model";
import { useTurnEvents } from "@/modules/tools/problems/stream";
import type { TurnDiagnosticsDto } from "@/shared/api/generated/ahab.schemas";
import type { AfterTurnProps } from "@/shared/chat-context/after-turn";
import { type ContextReference, referenceId } from "@/shared/chat-context/model";
import { t as translate, useTranslation } from "@/shared/i18n";
import { FileIcon } from "@/shared/ui/file-icon";

/** A turn's new problems as one piece of context for the agent. */
function turnReference(diagnostics: TurnDiagnosticsDto): ContextReference {
	return {
		id: referenceId(),
		kind: "searchResults",
		label: translate("tools:turnProblems.reference"),
		text: turnProblemsText(diagnostics),
	};
}

/**
 * After an agent's turn: "3 new errors, 1 fixed" in the files it
 * changed and their dependents, opening to the new problems by file, with
 * **Send to agent**, which puts them in the composer. Nothing is sent by
 * itself. Shows nothing when the turn changed no problems.
 */
export function TurnProblems({ chatId, turnId, onAddContext, onOpenFile }: AfterTurnProps) {
	const { t } = useTranslation("tools");
	const diagnostics = useTurnDiagnostics(chatId, turnId);
	useTurnEvents(chatId, turnId);
	const [open, setOpen] = useState(false);
	const [sent, setSent] = useState(false);
	const data = diagnostics.data;
	if (!data || (total(data.new) === 0 && total(data.fixed) === 0)) return null;
	const added = total(data.new);
	const files = data.files.filter((file) => file.new.length > 0);
	return (
		<section aria-label={t("turnProblems.section")} className="mt-2 text-sm">
			<div className="flex items-center gap-2">
				<button
					type="button"
					aria-expanded={open}
					disabled={files.length === 0}
					onClick={() => setOpen((now) => !now)}
					className="text-muted-foreground hover:text-foreground flex min-w-0 items-center gap-1 disabled:hover:text-inherit"
				>
					<ChevronRight
						className={cn(
							"size-3.5 shrink-0 transition-transform",
							open && "rotate-90",
							files.length === 0 && "invisible",
						)}
					/>
					<span className={cn("truncate", data.new.errors > 0 && "text-destructive")}>
						{turnText(data.new, data.fixed)}
					</span>
				</button>
				{added > 0 && onAddContext ? (
					<button
						type="button"
						onClick={() => {
							onAddContext([turnReference(data)]);
							setSent(true);
						}}
						title={t("turnProblems.sendHelp")}
						className="text-muted-foreground hover:text-foreground hover:bg-accent flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-xs"
					>
						<MessageSquarePlus className="size-3.5" />
						{sent ? t("turnProblems.sent") : t("turnProblems.send")}
					</button>
				) : null}
			</div>
			{open ? (
				<ul className="mt-1 flex flex-col gap-0.5">
					{files.map((file) => (
						<li key={file.path}>
							<p className="flex items-center gap-1 pl-5 text-xs font-medium" title={file.path}>
								<FileIcon path={file.path} className="size-3.5" />
								<span className="truncate">{file.path}</span>
							</p>
							<ul>
								{file.new.map((problem, index) => (
									<ProblemRow
										// oxlint-disable-next-line no-array-index-key -- a turn's record never changes
										key={index}
										problem={problem}
										onOpen={onOpenFile ? () => onOpenFile(problem.path, problem.line) : undefined}
									/>
								))}
							</ul>
						</li>
					))}
				</ul>
			) : null}
		</section>
	);
}
