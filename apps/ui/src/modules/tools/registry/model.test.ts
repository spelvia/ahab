import { describe, expect, it } from "vitest";

import {
	actionsOf,
	jobLine,
	machineProblem,
	matches,
	NO_FILTER,
	sortTools,
	stateWord,
	updatable,
	uptime,
	withJob,
} from "@/modules/tools/registry/model";
import type { ToolStatusDto } from "@/shared/api/generated/ahab.schemas";

function tool(overrides: Partial<ToolStatusDto> = {}): ToolStatusDto {
	return {
		id: "rust-analyzer",
		name: "rust-analyzer",
		kind: "languageServer",
		languages: ["rust"],
		origin: "ahab",
		state: "available",
		...overrides,
	};
}

const names: Record<string, string> = { rust: "Rust", python: "Python", typescript: "TypeScript" };
const nameOf = (id: string) => names[id] ?? id;

describe("tool states and actions", () => {
	it("says where a found tool is and why one cannot run", () => {
		expect(stateWord(tool({ state: "installed" }))).toBe("installed by Ahab");
		expect(stateWord(tool({ state: "found", detail: "on PATH (fnm)" }))).toBe("on PATH (fnm)");
		expect(
			stateWord(tool({ state: "cannotRun", reason: "needs Java 21 or later (found: Java 17)" })),
		).toBe("needs Java 21 or later (found: Java 17)");
		expect(stateWord(tool({ state: "missing", origin: "machine" }))).toBe("not found");
	});

	it("offers Install, Update and Remove only where they apply", () => {
		expect(actionsOf(tool({ state: "available" }))).toEqual({
			install: true,
			update: false,
			remove: false,
		});
		expect(actionsOf(tool({ state: "installed", updateAvailable: true }))).toEqual({
			install: false,
			update: true,
			remove: true,
		});
		expect(actionsOf(tool({ state: "found" }))).toEqual({
			install: false,
			update: false,
			remove: false,
		});
		const list = [
			tool({ id: "a", state: "installed", updateAvailable: true }),
			tool({ id: "b", state: "installed" }),
		];
		expect(updatable(list).map((item) => item.id)).toEqual(["a"]);
	});
});

describe("search, filters and order", () => {
	const ruff = tool({ id: "ruff", name: "ruff", kind: "linter", languages: ["python"] });
	const pyright = tool({
		id: "basedpyright",
		name: "basedpyright",
		languages: ["python"],
		default: true,
		state: "installed",
	});
	const ra = tool({ state: "found", default: true });

	it("finds tools by name and by language name", () => {
		expect(matches(ruff, { ...NO_FILTER, query: "pyth" }, nameOf)).toBe(true);
		expect(matches(ra, { ...NO_FILTER, query: "pyth" }, nameOf)).toBe(false);
		expect(matches(ruff, { ...NO_FILTER, kind: "linter", language: "python" })).toBe(true);
		expect(matches(ruff, { ...NO_FILTER, state: "present" })).toBe(false);
		expect(matches(pyright, { ...NO_FILTER, state: "present" })).toBe(true);
	});

	it("sorts by language, a language's defaults first", () => {
		expect(sortTools([ra, ruff, pyright], nameOf).map((item) => item.id)).toEqual([
			"basedpyright",
			"ruff",
			"rust-analyzer",
		]);
	});
});

describe("jobs", () => {
	it("keeps one job per tool, the newest", () => {
		const list = { platform: "linux-x64", runtimes: [], tools: [] };
		const first = withJob(list, {
			action: "install",
			state: "running",
			step: "download",
			tool: "ruff",
		});
		const next = withJob(first, {
			action: "install",
			state: "failed",
			step: "unpack",
			tool: "ruff",
			message: "bad checksum",
		});
		expect(next.jobs).toHaveLength(1);
		expect(jobLine(next.jobs![0]!)).toBe("Install failed at unpack: bad checksum");
		expect(jobLine(first.jobs![0]!)).toBe("Installing: download");
	});

	it("explains an outdated machine and counts uptime", () => {
		expect(machineProblem("machine_outdated", "conflict")).toBe(
			"Update Ahab on this machine for language tools.",
		);
		expect(machineProblem(undefined, "boom")).toBe("boom");
		const start = "2026-11-18T10:00:00Z";
		expect(uptime(start, Date.parse("2026-11-18T10:05:30Z"))).toBe("5 min");
		expect(uptime(start, Date.parse("2026-11-18T12:00:00Z"))).toBe("2 h");
	});
});
