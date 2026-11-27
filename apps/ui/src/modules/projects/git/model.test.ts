import { describe, expect, it } from "vitest";

import {
	groupCheckouts,
	allBut,
	applyLines,
	badges,
	hunks,
	hunkText,
	entries,
	fold,
	graph,
	sideBySide,
	stagedOf,
	statusLetter,
	tree,
} from "@/modules/projects/git/model";

describe("sideBySide", () => {
	it("faces removed lines with added ones and marks the changed words", () => {
		const rows = sideBySide("a\nold line\nc\n", "a\nnew line\nc\nd\n");
		expect(rows.map((row) => row.kind)).toEqual(["same", "change", "same", "add"]);
		const change = rows[1];
		if (change?.kind !== "change") throw new Error("expected a change");
		expect(change.left).toMatchObject({ line: 2, text: "old line" });
		expect(change.right).toMatchObject({ line: 2, text: "new line" });
		expect(change.left.pieces?.filter((p) => p.changed).map((p) => p.text)).toEqual(["old"]);
		expect(change.right.pieces?.filter((p) => p.changed).map((p) => p.text)).toEqual(["new"]);
		expect(rows[3]).toMatchObject({ kind: "add", right: { line: 4, text: "d" } });
	});

	it("shows new and deleted files on one side", () => {
		expect(sideBySide("", "x\ny\n").map((row) => row.kind)).toEqual(["add", "add"]);
		expect(sideBySide("x\n", "").map((row) => row.kind)).toEqual(["remove"]);
	});
});

function text(n: number): string {
	return Array.from({ length: n }, (_, i) => `line ${i + 1}`).join("\n");
}

describe("fold", () => {
	it("folds long unchanged runs, keeping lines around changes", () => {
		const before = `${text(20)}\n`;
		const after = `${text(20).replace("line 10", "line ten")}\n`;
		const rows = fold(sideBySide(before, after));
		expect(rows.map((row) => row.kind)).toEqual([
			"fold",
			"same",
			"same",
			"same",
			"change",
			"same",
			"same",
			"same",
			"fold",
		]);
		const first = rows[0];
		if (first?.kind !== "fold") throw new Error("expected a fold");
		expect(first.hidden).toBe(6);
		expect(fold(sideBySide(before, after), new Set([first.at]))).toHaveLength(14);
	});
});

describe("statusLetter", () => {
	it("names statuses as VS Code does", () => {
		expect(["modified", "added", "deleted", "untracked", "conflicted"].map(statusLetter)).toEqual([
			"M",
			"A",
			"D",
			"U",
			"!",
		]);
	});
});

function changed(path: string, status = "modified") {
	return { path, status };
}

describe("the one list of changes", () => {
	it("puts each file once, checked when staged and mixed when partly", () => {
		const list = entries({
			staged: [changed("b.ts"), changed("c.ts", "added")],
			changes: [changed("b.ts"), changed("a.ts")],
			untracked: [changed("d.ts", "untracked")],
		});
		expect(list.map((entry) => [entry.path, entry.staged, entry.group])).toEqual([
			["a.ts", "none", "changes"],
			["b.ts", "some", "changes"],
			["c.ts", "all", "staged"],
			["d.ts", "none", "untracked"],
		]);
		expect(stagedOf(list)).toBe("some");
		expect(stagedOf(list.filter((entry) => entry.path === "c.ts"))).toBe("all");
	});

	it("makes a tree with lone folders joined", () => {
		const nodes = tree([
			{ path: "src/app/shell/a.ts" },
			{ path: "src/app/shell/b.ts" },
			{ path: "README.md" },
			{ path: "src/main.ts" },
		]);
		expect(nodes.map((node) => (node.kind === "folder" ? node.name : node.entry.path))).toEqual([
			"src",
			"README.md",
		]);
		const src = nodes[0];
		if (src?.kind !== "folder") throw new Error("a folder");
		expect(src.files).toHaveLength(3);
		expect(
			src.children.map((node) => (node.kind === "folder" ? node.path : node.entry.path)),
		).toEqual(["src/app/shell", "src/main.ts"]);
	});
});

describe("the commit graph", () => {
	it("keeps a straight line on one lane", () => {
		const { rows, width } = graph([
			{ hash: "c", parents: ["b"] },
			{ hash: "b", parents: ["a"] },
			{ hash: "a", parents: [] },
		]);
		expect(width).toBe(1);
		expect(rows.map((row) => [row.lane, row.out, row.child])).toEqual([
			[0, [0], false],
			[0, [0], true],
			[0, [], true],
		]);
	});

	it("branches out at a merge and joins again at the fork", () => {
		// m merges f into b; f and b both come from a.
		const { rows, width } = graph([
			{ hash: "m", parents: ["b", "f"] },
			{ hash: "f", parents: ["a"] },
			{ hash: "b", parents: ["a"] },
			{ hash: "a", parents: [] },
		]);
		expect(width).toBe(2);
		expect(rows[0]).toMatchObject({ lane: 0, out: [0, 1] });
		expect(rows[1]).toMatchObject({ lane: 1, through: [0], out: [1] });
		// a is already awaited on lane 1: that line joins b's, which stays straight.
		expect(rows[2]).toMatchObject({ lane: 0, through: [], out: [0], joins: [1] });
		expect(rows[3]).toMatchObject({ lane: 0, into: [], out: [] });
	});

	it("shows refs as badges, the current branch first", () => {
		expect(
			badges(
				["HEAD", "feature/search", "origin/feature/search", "main", "tag: v1", "origin/dev"],
				["origin"],
			),
		).toEqual([
			{ name: "feature/search", kind: "head" },
			{ name: "main", kind: "branch" },
			{ name: "v1", kind: "tag" },
			{ name: "origin/dev", kind: "remote" },
		]);
		expect(badges(["HEAD", "tag: v2"])).toEqual([
			{ name: "HEAD", kind: "head" },
			{ name: "v2", kind: "tag" },
		]);
	});
});

describe("staging part of a file", () => {
	const before = "one\ntwo\nthree\nfour\n";
	const after = "one\n2\nthree\nfour\nfive\n";

	it("finds the hunks", () => {
		const rows = sideBySide(before, after);
		expect(hunks(rows)).toEqual([
			{ at: 1, removed: [2], added: [2] },
			{ at: 4, removed: [], added: [5] },
		]);
	});

	it("makes only the chosen changes", () => {
		expect(applyLines(before, after, { removed: new Set(), added: new Set([5]) })).toBe(
			"one\ntwo\nthree\nfour\nfive\n",
		);
		expect(applyLines(before, after, { removed: new Set([2]), added: new Set([2]) })).toBe(
			"one\n2\nthree\nfour\n",
		);
		expect(applyLines(before, after, { removed: new Set([2]), added: new Set() })).toBe(
			"one\nthree\nfour\n",
		);
		// Line endings stay as they were.
		expect(
			applyLines("a\r\nb\r\n", "a\r\nB\r\n", { removed: new Set([2]), added: new Set([2]) }),
		).toBe("a\r\nB\r\n");
	});

	it("unstages by taking every other change", () => {
		const rows = sideBySide(before, after);
		const keep = allBut(rows, { removed: new Set(), added: new Set([5]) });
		expect(applyLines(before, after, keep)).toBe("one\n2\nthree\nfour\n");
	});
});

describe("groupCheckouts", () => {
	it("puts a repository's worktrees together, and keeps lone repositories apart", () => {
		const groups = groupCheckouts([
			{ path: "api", branch: "main", repository: "api" },
			{ path: "clients/app/fix", branch: "fix", repository: "clients/app" },
			{ path: "clients/app/main", branch: "main", repository: "clients/app" },
			{ path: "svc", branch: "main", repository: "svc" },
			{ path: "svc-next", branch: "next", repository: "svc" },
			{ path: "old", branch: "main" },
		]);
		expect(groups.map((group) => [group.repository, group.checkouts.map((c) => c.label)])).toEqual([
			["api", ["api"]],
			["clients/app", ["fix", "main"]],
			["svc", ["svc", "svc-next"]],
			["old", ["old"]],
		]);
	});
});

describe("a hunk for an agent", () => {
	it("is its changed lines with a little context, and the new lines it covers", () => {
		const rows = sideBySide("a\nb\nc\nd\ne\n", "a\nb\nC\nd\ne\nf\n");
		const [first, second] = hunks(rows);
		if (!first || !second) throw new Error("two hunks expected");
		expect(hunkText(rows, first, 1)).toEqual({ text: " b\n-c\n+C\n d", startLine: 2, endLine: 4 });
		expect(hunkText(rows, second, 1)).toEqual({ text: " e\n+f", startLine: 5, endLine: 6 });
	});
});
