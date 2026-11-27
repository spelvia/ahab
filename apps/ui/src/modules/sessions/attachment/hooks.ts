import { useCallback, useEffect, useRef, useState } from "react";

import { useUploadAttachment } from "@/modules/sessions/attachment/api";
import { isImage, MAX_BYTES, MAX_FILES } from "@/modules/sessions/attachment/model";
import type { Attachment } from "@/shared/api/generated/ahab.schemas";
import { byteSize } from "@/shared/format";
import { t } from "@/shared/i18n";
import { composerDraft, rememberComposerDraft } from "@/shared/preferences/store";

/** A file being attached to the next message. */
interface Pending {
	key: string;
	name: string;
	size: number;
	/** A local preview, for images. */
	preview?: string;
	state: "uploading" | "ready" | "failed";
	attachment?: Attachment;
	error?: string;
}

/** The files for the next message, uploaded as soon as they are added. */
export interface Attachments {
	files: Pending[];
	add: (files: File[]) => void;
	remove: (key: string) => void;
	clear: () => void;
	/** The uploaded ones' IDs. */
	ids: string[];
	uploading: boolean;
	/** Files can be attached (there is a project to upload to). */
	enabled: boolean;
}

let nextFile = 0;

export function useAttachments(projectId: string | undefined, draftKey?: string): Attachments {
	const upload = useUploadAttachment();
	const [files, setFiles] = useState<Pending[]>(() => {
		const kept = draftKey ? composerDraft<Attachment[]>(draftKey, "attachments") : undefined;
		return (kept ?? []).map((attachment) => ({
			key: attachment.id,
			name: attachment.name,
			size: attachment.size,
			state: "ready",
			attachment,
		}));
	});
	const latest = useRef(files);
	const update = useCallback(
		(change: (list: Pending[]) => Pending[]) => {
			const next = change(latest.current);
			latest.current = next;
			setFiles(next);
			if (draftKey) {
				const kept = next.flatMap((file) => (file.attachment ? [file.attachment] : []));
				rememberComposerDraft(draftKey, "attachments", kept.length ? kept : undefined);
			}
		},
		[draftKey],
	);
	// Previews are released when the composer goes away.
	useEffect(
		() => () => {
			for (const file of latest.current) if (file.preview) URL.revokeObjectURL(file.preview);
		},
		[],
	);

	const change = useCallback(
		(key: string, patch: Partial<Pending>) => {
			update((list) => list.map((file) => (file.key === key ? { ...file, ...patch } : file)));
		},
		[update],
	);

	const add = useCallback(
		(added: File[]) => {
			if (!projectId) return;
			const room = MAX_FILES - latest.current.length;
			for (const [index, file] of added.entries()) {
				const key = `file-${nextFile++}`;
				const name = file.name || `pasted-${Date.now()}.png`;
				const entry: Pending = {
					key,
					name,
					size: file.size,
					preview: isImage(name) ? URL.createObjectURL(file) : undefined,
					state: "uploading",
				};
				if (index >= room) {
					entry.state = "failed";
					entry.error = t("sessions:attachments.tooMany", { count: MAX_FILES });
				} else if (file.size > MAX_BYTES) {
					entry.state = "failed";
					entry.error = t("sessions:attachments.tooLarge", { size: byteSize(file.size) });
				} else if (file.size === 0) {
					entry.state = "failed";
					entry.error = t("sessions:attachments.empty");
				}
				update((list) => [...list, entry]);
				if (entry.state === "failed") continue;
				// Each upload settles on its own (mutate's callbacks follow only the last).
				upload
					.mutateAsync({ id: projectId, data: file, params: { name } })
					.then((attachment) => change(key, { state: "ready", attachment }))
					.catch((error: Error) => change(key, { state: "failed", error: error.message }));
			}
		},
		[projectId, upload, change, update],
	);

	const remove = useCallback(
		(key: string) => {
			update((list) => {
				const gone = list.find((file) => file.key === key);
				if (gone?.preview) URL.revokeObjectURL(gone.preview);
				return list.filter((file) => file.key !== key);
			});
		},
		[update],
	);

	const clear = useCallback(() => {
		update((list) => {
			for (const file of list) if (file.preview) URL.revokeObjectURL(file.preview);
			return [];
		});
	}, [update]);

	return {
		files,
		add,
		remove,
		clear,
		ids: files.flatMap((file) => (file.attachment ? [file.attachment.id] : [])),
		uploading: files.some((file) => file.state === "uploading"),
		enabled: Boolean(projectId),
	};
}
