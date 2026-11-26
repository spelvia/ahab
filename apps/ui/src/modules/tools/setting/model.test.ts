import { describe, expect, it } from "vitest";

import {
	EMPTY_FORM,
	environmentOn,
	formFromRecord,
	moved,
	parseEnv,
	parseRecord,
	recordFromForm,
	recordOver,
	splitCommand,
	toggledWhen,
	toolsFor,
	withEnvironment,
	withLanguageTools,
	withoutRecord,
	withRecord,
	withTool,
} from "@/modules/tools/setting/model";

describe("a tool's machine settings", () => {
	it("reads NAME=value lines and says which are wrong", () => {
		expect(parseEnv("JAVA_HOME=/usr/lib/jvm/21\n\n# note\nA=b=c")).toEqual({
			env: { JAVA_HOME: "/usr/lib/jvm/21", A: "b=c" },
			errors: [],
		});
		expect(parseEnv("not a pair").errors).toEqual(["Line 1: write NAME=value."]);
	});

	it("stores only what is set, and drops a tool left with nothing", () => {
		const set = withTool({}, "jdtls", { path: "/opt/jdtls", env: {}, off: null });
		expect(set).toEqual({ tools: { jdtls: { path: "/opt/jdtls" } } });
		expect(withTool(set, "jdtls", { path: null })).toEqual({});
	});
});

describe("a project's languages", () => {
	it("shows the defaults until the project sets its own order", () => {
		expect(toolsFor(["biome", "eslint"], undefined)).toEqual({
			tools: ["biome", "eslint"],
			custom: false,
		});
		expect(toolsFor(["biome", "eslint"], { tools: ["eslint"] }).custom).toBe(true);
	});

	it("reorders, and forgets an order equal to the defaults", () => {
		expect(moved(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
		expect(moved(["a", "b"], 0, -1)).toEqual(["a", "b"]);
		const defaults = ["typescript-language-server", "biome"];
		const changed = withLanguageTools(
			{},
			"typescript",
			["biome", "typescript-language-server"],
			defaults,
		);
		expect(changed.languages?.typescript?.tools).toEqual(["biome", "typescript-language-server"]);
		expect(withLanguageTools(changed, "typescript", defaults, defaults)).toEqual({});
	});

	it("keeps the environment's defaults implicit", () => {
		expect(environmentOn(undefined, "direnv")).toBe(true);
		expect(environmentOn(undefined, "nix")).toBe(false);
		const nix = withEnvironment({}, { nix: true });
		expect(nix).toEqual({ environment: { nix: true } });
		expect(withEnvironment(nix, { nix: false })).toEqual({});
	});

	it("turns a check's times on and off in a fixed order", () => {
		expect(toggledWhen(undefined, [], "request", true)).toEqual(["request"]);
		expect(toggledWhen(["request"], [], "save", true)).toEqual(["save", "request"]);
		expect(toggledWhen(["save", "request"], [], "save", false)).toEqual(["request"]);
	});
});

describe("your own tools", () => {
	const form = {
		...EMPTY_FORM,
		id: "my-linter",
		name: "My linter",
		kind: "linter" as const,
		languages: "python, rust",
		command: 'my-linter --format "json lines"',
		rootMarkers: "pyproject.toml",
		output: "sarif" as const,
	};

	it("makes a record without a source from the form, and back", () => {
		const record = recordFromForm(form);
		expect(record).toEqual({
			id: "my-linter",
			name: "My linter",
			kind: "linter",
			languages: ["python", "rust"],
			run: {
				command: ["my-linter", "--format", "json lines"],
				rootMarkers: ["pyproject.toml"],
				output: { format: "sarif" },
			},
		});
		expect(formFromRecord(record)).toEqual(form);
		expect(splitCommand("a 'b c' d")).toEqual(["a", "b c", "d"]);
		const gradle = { ...form, kind: "check" as const, output: "checkstyle" as const };
		const reading = recordFromForm({ ...gradle, reports: "build/reports/checkstyle/*.xml" });
		expect(reading.run).toMatchObject({
			output: { format: "checkstyle", reports: ["build/reports/checkstyle/*.xml"] },
		});
		expect(formFromRecord(reading).reports).toBe("build/reports/checkstyle/*.xml");
	});

	it("keeps what the form does not show of an edited record", () => {
		const base = {
			id: "my-check",
			name: "Mine",
			kind: "check",
			languages: ["go"],
			run: { command: ["x"], env: { A: "1" }, output: { format: "json", json: { path: "p" } } },
		};
		const edited = recordOver({ ...formFromRecord(base), name: "Mine too" }, base);
		expect(edited.name).toBe("Mine too");
		expect(edited.run).toEqual(base.run);
	});

	it("says what is wrong with a typed record", () => {
		expect(parseRecord("{").errors[0]).toMatch(/^Not JSON/);
		expect(parseRecord('{"id":"X","source":{}}').errors).toEqual([
			"The ID is lower-case letters, digits and dashes.",
			"Give it a name.",
			"The kind is languageServer, formatter, linter or check.",
			"Name at least one language.",
			"Give its command.",
			"Your own tools have no source: Ahab finds them, never installs them.",
		]);
		expect(parseRecord(JSON.stringify(recordFromForm(form))).errors).toEqual([]);
	});

	it("adds, replaces under a new ID, and removes records", () => {
		const one = withRecord({}, { id: "a", name: "A" });
		const renamed = withRecord(one, { id: "b", name: "B" }, "a");
		expect(renamed.records).toEqual([{ id: "b", name: "B" }]);
		expect(withoutRecord(renamed, "b")).toEqual({});
	});
});
