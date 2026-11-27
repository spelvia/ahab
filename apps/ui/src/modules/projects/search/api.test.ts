import { describe, expect, it } from "vitest";

import { reduce } from "@/modules/projects/search/api";

const file = (path: string, lines: number) => ({
	path,
	more: false,
	matches: Array.from({ length: lines }, (_, line) => ({ line, preview: "x", ranges: [] })),
});

describe("a streamed search", () => {
	it("adds files as they come, ends with the total, and starts afresh", () => {
		let state = reduce({ searching: false, fresh: true }, { type: "start" });
		state = reduce(state, { type: "frame", frame: { type: "files", files: [file("a", 2)] } });
		state = reduce(state, { type: "frame", frame: { type: "files", files: [file("b", 1)] } });
		expect(state.searching).toBe(true);
		expect(state.data).toMatchObject({ total: 3, done: false });
		expect(state.data?.files.map((f) => f.path)).toEqual(["a", "b"]);
		state = reduce(state, { type: "frame", frame: { type: "done", total: 3, cut: "time" } });
		expect(state).toMatchObject({ searching: false, data: { total: 3, cut: "time", done: true } });

		// A new search keeps the old results shown until its first frame.
		state = reduce(state, { type: "start" });
		expect(state.data?.files).toHaveLength(2);
		state = reduce(state, { type: "frame", frame: { type: "files", files: [file("c", 1)] } });
		expect(state.data?.files.map((f) => f.path)).toEqual(["c"]);
	});
});
