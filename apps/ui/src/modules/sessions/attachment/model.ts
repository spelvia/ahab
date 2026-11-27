/** The largest attachment, as the server allows. */
export const MAX_BYTES = 10 * 1024 * 1024;
/** The most attachments in one message. */
export const MAX_FILES = 10;
/** The line before the attached files' paths in a message (server: attachment/model.rs). */
const HEADING = "[Attached files]";

/** A file attached to a sent message, from its path on the machine. */
export interface SentFile {
	id: string;
	name: string;
	path: string;
}

const ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A message's own text, and the files listed after it. */
export function splitAttachments(message: string): { text: string; files: SentFile[] } {
	const at = message.lastIndexOf(HEADING);
	if (at === -1) return { text: message, files: [] };
	const before = message.slice(0, at);
	if (before !== "" && !before.endsWith("\n\n")) return { text: message, files: [] };
	const files: SentFile[] = [];
	for (const path of message.slice(at + HEADING.length).split("\n")) {
		if (!path.trim()) continue;
		const parts = path.split(/[/\\]/);
		const name = parts.at(-1) ?? "";
		const id = parts.at(-2) ?? "";
		// Not a list Ahab wrote: show the message as it is.
		if (!ID.test(id) || !name) return { text: message, files: [] };
		files.push({ id, name, path });
	}
	return { text: before.trimEnd(), files };
}

/** Whether agents receive the file as an image. */
export function isImage(name: string): boolean {
	return /\.(png|jpe?g|gif|webp)$/i.test(name);
}
