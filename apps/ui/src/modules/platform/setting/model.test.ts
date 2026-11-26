import { describe, expect, it } from "vitest";

import { editorBehaviour, editorSettings } from "@/modules/platform/setting/model";

describe("editor settings", () => {
	it("take the server's values, and defaults for the rest", () => {
		expect(editorSettings({ fontSize: 15, vim: true })).toMatchObject({
			fontSize: 15,
			vim: true,
			tabSize: 4,
			lineNumbers: true,
		});
		expect(editorBehaviour({ autoSave: "afterDelay", editableByDefault: true })).toEqual({
			editableByDefault: true,
			autoSave: "afterDelay",
			autoSaveDelay: 1000,
		});
	});

	it("ignore values they do not know", () => {
		expect(editorBehaviour({ autoSave: "always" }).autoSave).toBe("off");
		expect(editorSettings({ fontSize: "big" }).fontSize).toBe(13);
		expect(editorSettings(undefined).lineWrapping).toBe(false);
	});
});
