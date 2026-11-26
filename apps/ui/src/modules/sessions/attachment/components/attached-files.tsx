import { attachmentUrl } from "@/modules/sessions/attachment/api";
import { isImage, type SentFile } from "@/modules/sessions/attachment/model";
import { useTranslation } from "@/shared/i18n";
import { FileIcon } from "@/shared/ui/file-icon";

/** The files sent with a message: images as thumbnails, others as links. */
export function AttachedFiles({ projectId, files }: { projectId: string; files: SentFile[] }) {
	const { t } = useTranslation("sessions");
	if (files.length === 0) return null;
	return (
		<ul aria-label={t("attachments.list")} className="flex flex-wrap justify-end gap-2">
			{files.map((file) => {
				const url = attachmentUrl(projectId, file.id, file.name);
				return (
					<li key={file.path}>
						<a
							href={url}
							target="_blank"
							rel="noreferrer"
							title={file.path}
							className="border-border bg-card hover:border-ring/60 focus-visible:ring-ring flex items-center gap-2 overflow-hidden rounded-lg border outline-none focus-visible:ring-2"
						>
							{isImage(file.name) ? (
								<img
									src={url}
									alt={file.name}
									loading="lazy"
									className="max-h-40 max-w-60 object-cover"
								/>
							) : (
								<span className="flex h-10 max-w-56 items-center gap-2 px-2.5 text-xs">
									<FileIcon path={file.name} />
									<span className="truncate">{file.name}</span>
								</span>
							)}
						</a>
					</li>
				);
			})}
		</ul>
	);
}
