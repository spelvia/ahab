import { cn } from "cn";
import { LoaderCircle, X } from "lucide-react";

import type { Attachments } from "@/modules/sessions/attachment/hooks";
import { byteSize } from "@/shared/format";
import { useTranslation } from "@/shared/i18n";
import { FileIcon } from "@/shared/ui/file-icon";

/** The files for the next message, above the text: a preview or icon, the name, ✕. */
export function AttachmentList({ attachments }: { attachments: Attachments }) {
	const { t } = useTranslation("sessions");
	if (attachments.files.length === 0) return null;
	return (
		<ul aria-label={t("attachments.list")} className="flex flex-wrap gap-2 px-1 pt-0.5 pb-2">
			{attachments.files.map((file) => (
				<li
					key={file.key}
					title={file.error ?? file.name}
					className={cn(
						"bg-background border-border relative flex h-12 max-w-56 items-center gap-2 rounded-lg border pr-7 pl-1",
						file.state === "failed" && "border-destructive/60",
					)}
				>
					{file.preview ? (
						<img src={file.preview} alt="" className="size-10 shrink-0 rounded-md object-cover" />
					) : (
						<span className="bg-muted grid size-10 shrink-0 place-items-center rounded-md">
							<FileIcon path={file.name} />
						</span>
					)}
					<span className="min-w-0">
						<span className="block truncate text-xs font-medium">{file.name}</span>
						<span
							className={cn(
								"flex items-center gap-1 text-[0.6875rem]",
								file.state === "failed" ? "text-destructive" : "text-muted-foreground",
							)}
						>
							{file.state === "uploading" ? <LoaderCircle className="size-3 animate-spin" /> : null}
							{file.state === "failed" ? t("attachments.notAttached") : byteSize(file.size)}
						</span>
					</span>
					<button
						type="button"
						onClick={() => attachments.remove(file.key)}
						aria-label={t("attachments.remove", { name: file.name })}
						className="text-muted-foreground hover:text-foreground hover:bg-accent absolute top-1 right-1 grid size-5 place-items-center rounded"
					>
						<X className="size-3.5" />
					</button>
				</li>
			))}
		</ul>
	);
}
