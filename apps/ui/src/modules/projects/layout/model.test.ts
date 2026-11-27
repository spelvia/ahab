import { describe, expect, it } from "vitest";

import { baseName, fileTabId, pathParts, withoutRetired } from "@/modules/projects/layout/model";

describe("tabs", () => {
	it("names a file tab after the file", () => {
		expect(baseName("crates/ahab/src/main.rs")).toBe("main.rs");
		expect(baseName("README.md")).toBe("README.md");
	});

	it("gives each file one tab", () => {
		expect(fileTabId("w1", "a/b.rs")).toBe(fileTabId("w1", "a/b.rs"));
		expect(fileTabId("w1", "a/b.rs")).not.toBe(fileTabId("w2", "a/b.rs"));
	});

	it("splits a path for its breadcrumb", () => {
		expect(pathParts("a/b/c.rs")).toEqual(["a", "b", "c.rs"]);
		expect(pathParts("")).toEqual([]);
	});
});

function leaf(id: string, views: string[], activeView?: string) {
	return { type: "leaf", size: 100, data: { id, views, activeView } };
}

describe("withoutRetired", () => {
	const saved = {
		grid: {
			root: {
				type: "branch",
				data: [leaf("1", ["file:a", "problems:w1"], "problems:w1"), leaf("2", ["problems:w2"])],
			},
			width: 800,
			height: 600,
			orientation: "HORIZONTAL",
		},
		panels: {
			"file:a": { id: "file:a", contentComponent: "file" },
			"problems:w1": { id: "problems:w1", contentComponent: "problems" },
			"problems:w2": { id: "problems:w2", contentComponent: "problems" },
		},
		activeGroup: "1",
	};

	it("takes out retired windows and their tabs", () => {
		const { layout, removed } = withoutRetired(saved, ["problems"]);
		expect(removed).toEqual(["problems"]);
		expect(layout).toEqual({
			...saved,
			grid: {
				...saved.grid,
				root: {
					type: "branch",
					data: [leaf("1", ["file:a"], "file:a"), leaf("2", [], undefined)],
				},
			},
			panels: { "file:a": saved.panels["file:a"] },
		});
	});

	it("leaves other layouts, and ones it cannot read, as they are", () => {
		const none = { ...saved, panels: { "file:a": saved.panels["file:a"] } };
		expect(withoutRetired(none, ["problems"]).layout).toBe(none);
		expect(withoutRetired(null, ["problems"])).toEqual({ layout: null, removed: [] });
		expect(withoutRetired("junk", ["problems"]).layout).toBe("junk");
		expect(withoutRetired(saved, []).layout).toBe(saved);
	});
});
