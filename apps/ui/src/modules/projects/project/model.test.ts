import { describe, expect, it } from "vitest";

import {
	groupProjects,
	inListOrder,
	readOnlyProjects,
	reordered,
	sharedNames,
} from "@/modules/projects/project/model";

const project = (id: string, name: string, group: string | null = null) => ({ id, name, group });

describe("groupProjects", () => {
	it("lists groups as they first come, the ungrouped ones last, each in the given order", () => {
		const groups = groupProjects([
			project("1", "ahab", "spelvia"),
			project("2", "homelab"),
			project("3", "acme", "work"),
			project("4", "stubb", "spelvia"),
			project("5", "notes"),
		]);
		expect(groups.map((entry) => [entry.group, entry.projects.map((p) => p.id)])).toEqual([
			["spelvia", ["1", "4"]],
			["work", ["3"]],
			[null, ["2", "5"]],
		]);
	});

	it("has only the ungrouped ones when no project has a group", () => {
		expect(groupProjects([project("1", "a")])).toEqual([
			{ group: null, projects: [project("1", "a")] },
		]);
	});
});

describe("sharedNames", () => {
	it("finds the projects whose name another has", () => {
		const shared = sharedNames([project("1", "app"), project("2", "app"), project("3", "other")]);
		expect([...shared]).toEqual(["1", "2"]);
	});
});

describe("readOnlyProjects", () => {
	it("holds every read-only project, the current one as it says itself", () => {
		const listed = [
			{ id: "here", mode: "readOnly" as const },
			{ id: "docs", mode: "readOnly" as const },
			{ id: "app", mode: "readWrite" as const },
		];
		expect(readOnlyProjects({ id: "here", mode: "readWrite" }, listed)).toEqual(["docs"]);
		expect(readOnlyProjects({ id: "app", mode: "readOnly" }, listed)).toEqual([
			"app",
			"here",
			"docs",
		]);
		expect(readOnlyProjects({ id: "here", mode: "readOnly" })).toEqual(["here"]);
	});
});

describe("project order", () => {
	const list = [
		{ id: "a", group: null },
		{ id: "w1", group: "Work" },
		{ id: "b", group: null },
		{ id: "w2", group: "Work" },
	];

	it("lists projects group by group, as the switcher shows them", () => {
		expect(inListOrder(list).map((listed) => listed.id)).toEqual(["w1", "w2", "a", "b"]);
	});

	it("moves a project within its group, by dragging or one step", () => {
		expect(reordered(list, "w2", "w1")).toEqual(["w2", "w1", "a", "b"]);
		expect(reordered(list, "a", 1)).toEqual(["w1", "w2", "b", "a"]);
	});

	it("never moves a project into another group or past the ends", () => {
		expect(reordered(list, "a", "w2")).toBeUndefined();
		expect(reordered(list, "w2", 1)).toBeUndefined();
		expect(reordered(list, "w1", -1)).toBeUndefined();
	});
});
