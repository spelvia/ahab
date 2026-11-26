import { cn } from "cn";
import { MessageSquarePlus, Paperclip } from "lucide-react";
import { useRef, useState } from "react";

import type { Attachments } from "@/modules/sessions/attachment/hooks";
import {
	carriesDraggedFiles,
	type ContextReference,
	draggedFiles,
	fileReference,
} from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";

function carriesFiles(event: React.DragEvent) {
	return event.dataTransfer.types.includes("Files");
}

/**
 * Files dragged anywhere over its children are attached; files dragged from
 * the file tree are added as context instead, when
 * `onReferences` takes them.
 */
export function DropZone({
	attachments,
	onReferences,
	className,
	children,
}: {
	attachments: Attachments;
	onReferences?: (references: ContextReference[]) => void;
	className?: string;
	children: React.ReactNode;
}) {
	const { t } = useTranslation("sessions");
	const [over, setOver] = useState<"attach" | "context" | null>(null);
	// Entering a child leaves the parent; count to know when it really left.
	const depth = useRef(0);
	const kind = (event: React.DragEvent) =>
		onReferences && carriesDraggedFiles(event.dataTransfer)
			? "context"
			: attachments.enabled && carriesFiles(event)
				? "attach"
				: null;
	// One shape whether or not it takes drops (its handlers then ignore them):
	// a different one would mount what it holds again as a chat loads.
	return (
		<div
			className={cn("relative", className)}
			onDragEnter={(event) => {
				const taken = kind(event);
				if (!taken) return;
				depth.current += 1;
				setOver(taken);
			}}
			onDragOver={(event) => {
				const taken = kind(event);
				if (!taken) return;
				event.preventDefault();
				// The tree allows copy and move: adding as context is a copy.
				event.dataTransfer.dropEffect = "copy";
			}}
			onDragLeave={(event) => {
				if (!kind(event)) return;
				depth.current = Math.max(0, depth.current - 1);
				if (depth.current === 0) setOver(null);
			}}
			onDrop={(event) => {
				const taken = kind(event);
				if (!taken) return;
				event.preventDefault();
				depth.current = 0;
				setOver(null);
				if (taken === "context") {
					const dragged = draggedFiles(event.dataTransfer);
					// Whole files only: an agent is pointed at a folder some other way.
					const files = dragged?.paths.filter((path) => !dragged.folders?.includes(path)) ?? [];
					if (dragged && files.length)
						onReferences?.(files.map((path) => fileReference(dragged.projectId, path)));
					return;
				}
				attachments.add([...event.dataTransfer.files]);
			}}
		>
			{children}
			{over ? (
				<div className="bg-background/80 border-primary text-heading pointer-events-none absolute inset-2 z-20 grid place-items-center rounded-xl border-2 border-dashed">
					<span className="flex items-center gap-2 text-sm font-medium">
						{over === "context" ? (
							<>
								<MessageSquarePlus className="size-4" />
								{t("attachments.dropAsContext")}
							</>
						) : (
							<>
								<Paperclip className="size-4" />
								{t("attachments.dropToAttach")}
							</>
						)}
					</span>
				</div>
			) : null}
		</div>
	);
}
