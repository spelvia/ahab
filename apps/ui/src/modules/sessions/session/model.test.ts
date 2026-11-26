import { describe, expect, it } from "vitest";

import {
	folderWithin,
	selectRange,
	sessionGroups,
	sessionTree,
} from "@/modules/sessions/session/model";
import type { Session, SessionFolder } from "@/shared/api/generated/ahab.schemas";

const folder = (
	id: string,
	name: string,
	parentId: string | null = null,
	pinnedAt: string | null = null,
): SessionFolder => ({
	id,
	projectId: "p",
	name,
	parentId,
	pinnedAt,
	createdAt: "2026-11-19T09:00:00Z",
	updatedAt: "2026-11-19T09:00:00Z",
});

const session = (id: string, change: Partial<Session> = {}): Session => ({
	id,
	kind: "chat",
	title: id,
	projectId: "p",
	state: "idle",
	waiting: 0,
	chats: [],
	terminal: null,
	terminals: [],
	folderId: null,
	pinnedAt: null,
	archivedAt: null,
	createdAt: "2026-11-19T09:00:00Z",
	updatedAt: "2026-11-19T09:00:00Z",
	...change,
});

describe("the Sessions view's tree", () => {
	it("nests folders, pinned first, then by name and by latest activity", () => {
		const tree = sessionTree(
			[
				session("old", { updatedAt: "2026-11-19T08:00:00Z" }),
				session("new", { updatedAt: "2026-11-19T10:00:00Z" }),
				session("pinned", { pinnedAt: "2026-11-19T07:00:00Z", updatedAt: "2026-11-01T00:00:00Z" }),
				session("bug", { folderId: "bugs", state: "needsYou" }),
				session("lost", { folderId: "gone" }),
			],
			[
				folder("work", "Work"),
				folder("bugs", "Bugs", "work"),
				folder("alpha", "Alpha"),
				folder("zed", "Zed", null, "2026-11-19T06:00:00Z"),
			],
		);
		expect(tree.folders.map((node) => node.folder.name)).toEqual(["Zed", "Alpha", "Work"]);
		const work = tree.folders[2]!;
		expect(work.folders.map((node) => node.folder.name)).toEqual(["Bugs"]);
		expect(work.waiting).toBe(1);
		expect(work.count).toBe(1);
		expect(tree.sessions.map((item) => item.id)).toEqual(["pinned", "new", "lost", "old"]);
	});

	it("puts a folder in a loop, or under a folder that is gone, at the top", () => {
		const tree = sessionTree(
			[],
			[folder("a", "A", "b"), folder("b", "B", "a"), folder("c", "C", "gone")],
		);
		expect(tree.folders.map((node) => node.folder.name)).toEqual(["A", "B", "C"]);
	});

	it("knows a folder cannot go into itself or its own folders", () => {
		const all = [folder("work", "Work"), folder("bugs", "Bugs", "work"), folder("other", "Other")];
		expect(folderWithin(all, "bugs", "work")).toBe(true);
		expect(folderWithin(all, "work", "work")).toBe(true);
		expect(folderWithin(all, "other", "work")).toBe(false);
	});
});

const label = (_grouping: string, key: string) => key;
const ids = (groups: { key: string; sessions: Session[] }[]) =>
	groups.map((group) => [group.key, group.sessions.map((item) => item.id)]);

describe("sorting and grouping sessions", () => {
	const now = new Date("2026-11-23T12:00:00");
	const list = [
		session("b", {
			title: "beta",
			state: "ended",
			updatedAt: "2026-11-23T10:00:00",
			createdAt: "2026-11-01T00:00:00Z",
		}),
		session("a", {
			title: "Alpha",
			state: "needsYou",
			updatedAt: "2026-11-22T10:00:00",
			createdAt: "2026-11-02T00:00:00Z",
		}),
		session("c", {
			title: "gamma",
			state: "working",
			updatedAt: "2026-10-01T10:00:00",
			createdAt: "2026-11-03T00:00:00Z",
		}),
	];

	it("sorts by activity, creation or name", () => {
		expect(ids(sessionGroups(list, "none", "activity", label, now))).toEqual([
			["all", ["b", "a", "c"]],
		]);
		expect(ids(sessionGroups(list, "none", "created", label, now))).toEqual([
			["all", ["c", "a", "b"]],
		]);
		expect(ids(sessionGroups(list, "none", "name", label, now))).toEqual([
			["all", ["a", "b", "c"]],
		]);
	});

	it("groups by status (needing you first) and by day", () => {
		expect(ids(sessionGroups(list, "status", "activity", label, now))).toEqual([
			["needsYou", ["a"]],
			["working", ["c"]],
			["ended", ["b"]],
		]);
		expect(ids(sessionGroups(list, "day", "activity", label, now))).toEqual([
			["today", ["b"]],
			["yesterday", ["a"]],
			["older", ["c"]],
		]);
	});

	it("selects a range in either direction", () => {
		const listed = ["a", "b", "c", "d"];
		expect(selectRange(listed, "b", "d")).toEqual(["b", "c", "d"]);
		expect(selectRange(listed, "d", "b")).toEqual(["b", "c", "d"]);
		expect(selectRange(listed, "gone", "c")).toEqual(["c"]);
	});
});
