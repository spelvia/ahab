import { preview, ReferenceChip } from "@/modules/sessions/reference/components/reference-chip";
import { useDraft, draftStore } from "@/modules/sessions/reference/hooks";
import { referenceLabel } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";

/** The context a composer holds, sent with its next message; each removable. */
export function DraftReferences({
	draftKey,
	onOpenFile,
}: {
	draftKey: string;
	onOpenFile?: (path: string, line?: number) => void;
}) {
	const { t } = useTranslation("sessions");
	const list = useDraft(draftKey);
	if (list.length === 0) return null;
	return (
		<ul aria-label={t("references.draft")} className="mb-1.5 flex flex-wrap gap-1.5 px-1">
			{list.map((reference) => (
				<li key={reference.id}>
					<ReferenceChip
						kind={reference.kind}
						label={referenceLabel(reference)}
						path={reference.path}
						title={preview(reference.text) ?? reference.path}
						onOpen={
							reference.path && onOpenFile
								? () => onOpenFile(reference.path ?? "", reference.startLine)
								: undefined
						}
						onRemove={() => draftStore.remove(draftKey, [reference.id])}
					/>
				</li>
			))}
		</ul>
	);
}
