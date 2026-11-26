import { describe, expect, it } from "vitest";

import {
	ALL_SEVERITIES,
	allProblems,
	countOf,
	countText,
	filtering,
	globToRegExp,
	groupByFile,
	checkoutState,
	groupByRepository,
	matches,
	panelMatches,
	parseFilter,
	problemLine,
	sourcesOf,
	turnText,
} from "@/modules/tools/problems/model";
import type { ProblemDto } from "@/shared/api/generated/ahab.schemas";

function problem(change: Partial<ProblemDto>): ProblemDto {
	return {
		path: "src/main.rs",
		source: "rust-analyzer",
		severity: "error",
		line: 1,
		column: 1,
		endLine: 1,
		endColumn: 2,
		code: null,
		message: "mismatched types",
		...change,
	};
}

const none = { errors: 0, warnings: 0, information: 0, hints: 0 };

describe("problems", () => {
	it("groups by file, the worst file first, each in order of place", () => {
		const groups = groupByFile([
			problem({ path: "b.rs", severity: "warning", line: 9 }),
			problem({ path: "c.rs", severity: "error", line: 4 }),
			problem({ path: "b.rs", severity: "hint", line: 2 }),
			problem({ path: "a.rs", severity: "warning" }),
		]);
		expect(groups.map((group) => group.path)).toEqual(["c.rs", "a.rs", "b.rs"]);
		expect(groups[2]?.problems.map((item) => item.line)).toEqual([2, 9]);
		expect(groups[2]?.counts).toEqual({ errors: 0, warnings: 1, information: 0, hints: 1 });
	});

	it("filters by lowest severity, tool and text", () => {
		const warning = problem({ severity: "warning", source: "clippy", code: "needless_return" });
		const hint = problem({ severity: "hint", message: "unused import" });
		expect(matches(hint, allProblems)).toBe(true);
		expect(matches(hint, { ...allProblems, severity: "warning" })).toBe(false);
		expect(matches(warning, { ...allProblems, severity: "warning" })).toBe(true);
		expect(matches(warning, { ...allProblems, source: "rust-analyzer" })).toBe(false);
		expect(matches(warning, { ...allProblems, text: "NEEDLESS" })).toBe(true);
		expect(matches(hint, { ...allProblems, text: "main.rs" })).toBe(true);
		expect(matches(hint, { ...allProblems, text: "borrow" })).toBe(false);
		expect(sourcesOf([warning, hint, warning])).toEqual(["clippy", "rust-analyzer"]);
	});

	it("counts in words", () => {
		const counts = countOf([
			problem({}),
			problem({}),
			problem({ severity: "warning" }),
			problem({ severity: "information" }),
		]);
		expect(countText(counts)).toBe("2 errors, 1 warning, 1 info");
		expect(countText(none)).toBe("No problems");
	});

	it("says what a turn changed", () => {
		expect(turnText({ ...none, errors: 3 }, { ...none, errors: 1 })).toBe("3 new errors, 1 fixed");
		expect(turnText({ ...none, warnings: 1 }, none)).toBe("1 new warning");
		expect(turnText(none, { ...none, errors: 1, warnings: 1 })).toBe("2 fixed");
	});

	it("writes a problem on one line, as tools do", () => {
		expect(problemLine(problem({ line: 12, column: 5, code: "E0308" }))).toBe(
			"src/main.rs:12:5: error[E0308]: mismatched types (rust-analyzer)",
		);
	});
});

function at(path: string, change: Partial<ProblemDto> = {}) {
	return problem({ path, ...change });
}

function shows(text: string, item: ProblemDto, severities = ALL_SEVERITIES, source = "") {
	return panelMatches(item, { filter: parseFilter(text), severities, source });
}

describe("the panel's filter", () => {
	it("reads globs as VS Code does", () => {
		expect(globToRegExp("**/*.ts").test("src/a.ts")).toBe(true);
		expect(globToRegExp("**/*.ts").test("a.ts")).toBe(true);
		expect(globToRegExp("**/*.ts").test("src/a.tsx")).toBe(false);
		expect(globToRegExp("*.ts").test("deep/down/a.ts")).toBe(true);
		expect(globToRegExp("src/*.ts").test("src/a.ts")).toBe(true);
		expect(globToRegExp("src/*.ts").test("src/x/a.ts")).toBe(false);
		expect(globToRegExp("src/**/*.ts").test("src/a.ts")).toBe(true);
		expect(globToRegExp("**/*.{ts,tsx}").test("ui/app.tsx")).toBe(true);
		expect(globToRegExp("**/test/**").test("test/a.rs")).toBe(true);
		expect(globToRegExp("**/test/**").test("crates/x/test/a.rs")).toBe(true);
		expect(globToRegExp("**/test/**").test("crates/latest/a.rs")).toBe(false);
		expect(globToRegExp("file?.rs").test("file1.rs")).toBe(true);
		expect(globToRegExp("[ab].rs").test("b.rs")).toBe(true);
		expect(globToRegExp("ui/src").test("ui/src/main.ts")).toBe(true);
		expect(globToRegExp("a.b").test("axb")).toBe(false);
	});

	it("keeps words and globs, and leaves out what ! names", () => {
		const ts = at("src/app.ts", { message: "Type 'string' is not assignable" });
		const test = at("src/test/app.ts", { message: "unused variable" });
		const rs = at("crates/main.rs", { code: "E0308" });
		expect(shows("", ts)).toBe(true);
		expect(shows("**/*.ts", rs)).toBe(false);
		expect(shows("**/*.ts", ts)).toBe(true);
		expect(shows("**/*.ts, !**/test/**", test)).toBe(false);
		expect(shows("**/*.ts, !**/test/**", ts)).toBe(true);
		expect(shows("assignable", ts)).toBe(true);
		expect(shows("ASSIGNABLE", test)).toBe(false);
		expect(shows("e0308", rs)).toBe(true);
		expect(shows("unused, assignable", test)).toBe(true);
		expect(shows("!unused", test)).toBe(false);
		expect(shows("**/*.ts, unused", ts)).toBe(false);
	});

	it("shows the severities asked for, and one tool", () => {
		const warning = at("a.rs", { severity: "warning", source: "clippy" });
		const hint = at("a.rs", { severity: "hint" });
		expect(shows("", warning, { ...ALL_SEVERITIES, warnings: false })).toBe(false);
		expect(shows("", hint, { ...ALL_SEVERITIES, infos: false })).toBe(false);
		expect(shows("", hint, { ...ALL_SEVERITIES, errors: false })).toBe(true);
		expect(shows("", warning, ALL_SEVERITIES, "rust-analyzer")).toBe(false);
		expect(shows("", warning, ALL_SEVERITIES, "clippy")).toBe(true);
		expect(filtering({ filter: parseFilter(" , "), severities: ALL_SEVERITIES, source: "" })).toBe(
			false,
		);
		expect(filtering({ filter: parseFilter("x"), severities: ALL_SEVERITIES, source: "" })).toBe(
			true,
		);
	});

	it("groups by repository, then worktree, then file; the rest are not checked", () => {
		const checkouts = [
			{ path: "app/main", repository: "app", kind: "worktree" },
			{ path: "app/fix", repository: "app", kind: "worktree" },
			{ path: "svc", repository: "svc", kind: "repository" },
			{ path: "idle", repository: "idle", kind: "repository" },
			{ path: "pinned", repository: "pinned", kind: "repository" },
		];
		const activity = [
			{ repository: "app", worktree: "app/fix", pinned: false, active: true },
			{ repository: "pinned", worktree: null, pinned: true, active: true },
		];
		const grouped = groupByRepository(
			[
				at("app/main/src/A.java"),
				at("app/fix/src/B.java", { severity: "warning" }),
				at("svc/pom.xml"),
				at("README.md", { severity: "hint" }),
			],
			checkouts,
			activity,
		);
		expect(grouped.repositories.map((group) => group.repository)).toEqual(["app", "svc"]);
		const app = grouped.repositories[0]!;
		expect(app.single).toBe(false);
		expect(app.checkouts.map((c) => [c.label, c.active, c.worktree])).toEqual([
			["main", false, true],
			["fix", true, true],
		]);
		expect(app.counts).toEqual({ ...none, errors: 1, warnings: 1 });
		expect(grouped.repositories[1]!.single).toBe(true);
		expect(grouped.loose.map((file) => file.path)).toEqual(["README.md"]);
		// Active or pinned without problems: nothing to list, but checked.
		expect(grouped.notChecked.map((c) => c.path)).toEqual(["idle"]);
		expect(
			checkoutState(checkouts[0]!, [{ repository: "app", pinned: true, active: true }]),
		).toEqual({
			pinned: true,
			active: true,
		});
	});
});
