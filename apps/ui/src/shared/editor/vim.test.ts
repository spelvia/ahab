import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
	pendingKeys,
	publishVimStatus,
	useVimStatus,
	vimModeLabel,
	vimModeOf,
} from "@/shared/editor/vim";

describe("Vim's status", () => {
	it("names the modes as Vim's status line does", () => {
		expect(vimModeOf({ mode: "normal" })).toBe("normal");
		expect(vimModeOf({ mode: "insert" })).toBe("insert");
		expect(vimModeOf({ mode: "replace" })).toBe("replace");
		expect(vimModeOf({ mode: "visual", subMode: "" })).toBe("visual");
		expect(vimModeOf({ mode: "visual", subMode: "linewise" })).toBe("visualLine");
		expect(vimModeOf({ mode: "visual", subMode: "blockwise" })).toBe("visualBlock");
		expect(vimModeOf({ mode: "something new" })).toBe("normal");
		expect(vimModeLabel("normal")).toBe("NORMAL");
		expect(vimModeLabel("visualLine")).toBe("VISUAL LINE");
		expect(vimModeLabel("visualBlock")).toBe("VISUAL BLOCK");
		expect(vimModeLabel("replace")).toBe("REPLACE");
	});

	it("shows the keys of a command while it is pending, as showcmd does", () => {
		expect(
			pendingKeys("normal", "d2", { operator: "delete", prefixRepeat: [], keyBuffer: [] }),
		).toBe("d2");
		expect(pendingKeys("normal", "g", { keyBuffer: ["g"] })).toBe("g");
		expect(pendingKeys("normal", '"a', { registerName: "a" })).toBe('"a');
		expect(pendingKeys("visual", "3", { prefixRepeat: ["3"] })).toBe("3");
		// Done, or typing text: nothing.
		expect(pendingKeys("normal", "dw", { operator: null, keyBuffer: [] })).toBe("");
		expect(pendingKeys("insert", "abc", { keyBuffer: ["a"] })).toBe("");
		expect(pendingKeys("normal", "x", undefined)).toBe("");
		expect(pendingKeys("normal", "123456789012", { prefixRepeat: ["1"] })).toBe("3456789012");
	});

	it("follows the editor that published last, until it is gone", () => {
		const { result } = renderHook(() => useVimStatus());
		const one = {};
		const two = {};
		act(() => publishVimStatus(one, { mode: "insert", pending: "" }));
		expect(result.current).toEqual({ mode: "insert", pending: "" });
		act(() => publishVimStatus(two, { mode: "normal", pending: "d" }));
		expect(result.current).toEqual({ mode: "normal", pending: "d" });
		// An editor no longer shown takes back only its own.
		act(() => publishVimStatus(one, null));
		expect(result.current).toEqual({ mode: "normal", pending: "d" });
		act(() => publishVimStatus(two, null));
		expect(result.current).toBeNull();
	});
});
