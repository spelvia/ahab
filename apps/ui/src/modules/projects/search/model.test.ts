import { describe, expect, it } from "vitest";

import { fuzzy, globs, rank } from "@/modules/projects/search/model";

describe("quick open", () => {
	const paths = [
		"ui/src/app/shell/workbench-tiles.tsx",
		"ui/src/app/shell/workbench.tsx",
		"crates/ahab/src/server/web.rs",
		"README.md",
	];

	it("matches letters in order and ranks file names and word starts first", () => {
		expect(fuzzy("wbtiles", paths[0] ?? "")).toBeDefined();
		expect(fuzzy("zzz", paths[0] ?? "")).toBeUndefined();
		expect(rank("wbtiles", paths)[0]?.path).toBe("ui/src/app/shell/workbench-tiles.tsx");
		expect(rank("readme", paths)[0]?.path).toBe("README.md");
		expect(rank("workbench", paths)[0]?.path).toBe("ui/src/app/shell/workbench.tsx");
	});

	it("reads glob lists", () => {
		expect(globs(" src/**, *.rs ,,")).toEqual(["src/**", "*.rs"]);
	});
});
