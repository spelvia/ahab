import { describe, expect, it } from "vitest";

import { contributedLanguagesOf } from "@/modules/plugins/runtime/model";

describe("a plugin's languages", () => {
	it("are read from its manifest, their files served from the plugin", () => {
		const languages = contributedLanguagesOf("zig", "abc", {
			contributes: {
				languages: [
					{
						id: "zig",
						name: "Zig",
						files: { extensions: ["zig"] },
						grammar: { wasm: "grammars/zig.wasm" },
						queries: { highlights: "queries/highlights.scm" },
					},
					{ name: "no id" },
				],
			},
		});
		expect(languages).toHaveLength(1);
		expect(languages[0]).toMatchObject({
			id: "zig",
			source: "zig",
			wasm: "/api/v1/plugins/zig/files/grammars/zig.wasm?v=abc",
			queries: { highlights: "/api/v1/plugins/zig/files/queries/highlights.scm?v=abc" },
		});
	});

	it("are none without contributions", () => {
		expect(contributedLanguagesOf("echo", "abc", { entries: { web: "dist/web.js" } })).toEqual([]);
		expect(contributedLanguagesOf("echo", "abc", null)).toEqual([]);
	});
});
