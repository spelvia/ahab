import { preview, ReferenceChip } from "@/modules/sessions/reference/components/reference-chip";
import type { SentReference } from "@/modules/sessions/reference/model";
import { linesLabel } from "@/shared/editor/model";
import { i18n, useTranslation } from "@/shared/i18n";

function label(reference: SentReference): string {
	const path = reference.path ?? "";
	const lines =
		reference.startLine !== undefined
			? linesLabel(path, reference.startLine, reference.endLine ?? reference.startLine)
			: path;
	switch (reference.kind) {
		case "diffHunk":
			return i18n.t("sessions:references.diff", { lines });
		case "terminalOutput":
			return reference.label
				? i18n.t("sessions:references.terminalFrom", { label: reference.label })
				: i18n.t("sessions:references.terminal");
		case "searchResults":
			return reference.label
				? i18n.t("sessions:references.searchFor", { label: reference.label })
				: i18n.t("sessions:references.search");
		case "chatHistory":
			return reference.label
				? i18n.t("sessions:references.historyFrom", { label: reference.label })
				: i18n.t("sessions:references.history");
		case "other":
			return reference.label ?? path;
		default:
			return lines;
	}
}

/** The context a sent message carried, as chips; files open at their lines. */
export function SentReferences({
	references,
	onOpenFile,
}: {
	references: SentReference[];
	onOpenFile?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation("sessions");
	if (references.length === 0) return null;
	return (
		<ul
			aria-label={t("references.sent")}
			className="flex max-w-[85%] flex-wrap justify-end gap-1.5"
		>
			{references.map((reference, index) => (
				// oxlint-disable-next-line no-array-index-key -- a sent message never changes
				<li key={index}>
					<ReferenceChip
						kind={reference.kind}
						label={label(reference)}
						path={reference.path}
						stale={reference.stale}
						title={preview(reference.text)}
						onOpen={
							reference.path && onOpenFile
								? () => onOpenFile(reference.path ?? "", reference.startLine)
								: undefined
						}
					/>
				</li>
			))}
		</ul>
	);
}
