import { SeverityIcon } from "@/modules/tools/problems/components/severity-icon";
import type { ProblemDto } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";

/** One problem: its severity, message, tool and code, and where it is. */
export function ProblemRow({ problem, onOpen }: { problem: ProblemDto; onOpen?: () => void }) {
	const { t } = useTranslation("tools");
	const body = (
		<>
			<SeverityIcon severity={problem.severity} className="mt-0.5" />
			<span className="min-w-0 flex-1">
				<span className="break-words whitespace-pre-wrap">{problem.message}</span>{" "}
				<span className="text-muted-foreground text-[0.6875rem]">
					{problem.source}
					{problem.code ? `(${problem.code})` : ""}{" "}
					{t("problemRow.position", { line: problem.line, column: problem.column })}
				</span>
			</span>
		</>
	);
	return (
		<li>
			{onOpen ? (
				<button
					type="button"
					onClick={onOpen}
					title={t("problemRow.open", { path: problem.path, line: problem.line })}
					className="hover:bg-accent/60 focus-visible:bg-accent flex w-full items-start gap-1.5 rounded py-0.5 pr-1 pl-6 text-left text-xs outline-none"
				>
					{body}
				</button>
			) : (
				<div className="flex items-start gap-1.5 py-0.5 pr-1 pl-6 text-xs">{body}</div>
			)}
		</li>
	);
}
