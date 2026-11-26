import { describe, expect, it } from "vitest";

import { isImage, splitAttachments } from "@/modules/sessions/attachment/model";
import { byteSize as size } from "@/shared/format";

const id = "01a1db75-1215-70c0-9cdb-476701bc6899";

describe("splitAttachments", () => {
	it("separates the text from the files listed after it", () => {
		const message = `Look at this\n\n[Attached files]\n/data/attachments/${id}/shot.png\n/data/attachments/${id}/notes.txt`;
		expect(splitAttachments(message)).toEqual({
			text: "Look at this",
			files: [
				{ id, name: "shot.png", path: `/data/attachments/${id}/shot.png` },
				{ id, name: "notes.txt", path: `/data/attachments/${id}/notes.txt` },
			],
		});
		expect(splitAttachments(`[Attached files]\n/a/${id}/x.png`).text).toBe("");
	});

	it("leaves messages alone that only mention the heading", () => {
		expect(splitAttachments("hi").files).toEqual([]);
		const quoted = "What does [Attached files]\n/etc/passwd mean?";
		expect(splitAttachments(quoted)).toEqual({ text: quoted, files: [] });
		const fake = "x\n\n[Attached files]\n/etc/passwd";
		expect(splitAttachments(fake)).toEqual({ text: fake, files: [] });
	});
});

describe("files", () => {
	it("knows images and says sizes briefly", () => {
		expect(isImage("a.JPG")).toBe(true);
		expect(isImage("a.pdf")).toBe(false);
		expect(size(900)).toBe("900 B");
		expect(size(12 * 1024)).toBe("12 KB");
		expect(size(3.4 * 1024 * 1024)).toBe("3.4 MB");
	});
});
