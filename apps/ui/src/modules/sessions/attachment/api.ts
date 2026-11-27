import { useUploadAttachment as useGeneratedUpload } from "@/shared/api/generated/sessions-attachment/sessions-attachment";

/** Uploads a file to the project's machine, for the next message. */
export function useUploadAttachment() {
	return useGeneratedUpload();
}

/** Where a kept attachment is read from (thumbnails, opening it). */
export function attachmentUrl(projectId: string, id: string, name: string): string {
	return `/api/v1/projects/${projectId}/attachments/${id}/${encodeURIComponent(name)}`;
}
