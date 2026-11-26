import { describe, expect, it } from "vitest";

import {
	appliedVimMapping,
	normalizeVimKeys,
	type VimMapping,
	vimMappingProblem,
} from "@/shared/keymap/vim";

const mapping = (lhs: string, rhs: string, mode: VimMapping["mode"] = "normal"): VimMapping => ({
	mode,
	lhs,
	rhs,
});

describe("Vim mappings", () => {
	it("writes keys as the editor's Vim reads them", () => {
		expect(normalizeVimKeys("jk")).toEqual({ keys: "jk" });
		expect(normalizeVimKeys("<esc>")).toEqual({ keys: "<Esc>" });
		expect(normalizeVimKeys("<Enter>")).toEqual({ keys: "<CR>" });
		expect(normalizeVimKeys("<s-c-x>")).toEqual({ keys: "<C-S-x>" });
		expect(normalizeVimKeys(":w<cr>")).toEqual({ keys: ":w<CR>" });
		expect(normalizeVimKeys("<C-->")).toEqual({ keys: "<C-->" });
		expect(normalizeVimKeys("<f5>")).toEqual({ keys: "<F5>" });
		// A lone `<` is the key itself.
		expect(normalizeVimKeys("<")).toEqual({ keys: "<" });
		expect(normalizeVimKeys("<Leader>w")).toEqual({ unknown: "<Leader>" });
		expect(normalizeVimKeys("<X-a>")).toEqual({ unknown: "<X-a>" });
	});

	it("says what keeps a mapping from being applied", () => {
		expect(vimMappingProblem(mapping("jk", "<Esc>", "insert"), [])).toBeUndefined();
		expect(vimMappingProblem(mapping("<C-s>", ":w<CR>"), [])).toBeUndefined();
		expect(vimMappingProblem(mapping(" ", "x"), [])).toEqual({ problem: "lhsEmpty" });
		expect(vimMappingProblem(mapping("x", ""), [])).toEqual({ problem: "rhsEmpty" });
		expect(vimMappingProblem(mapping(":W", ":w"), [])).toEqual({ problem: "exCommand" });
		expect(vimMappingProblem(mapping("j", "<Esc>", "insert"), [])).toEqual({ problem: "typing" });
		expect(vimMappingProblem(mapping("<C-j>", "<Esc>", "insert"), [])).toBeUndefined();
		expect(vimMappingProblem(mapping("<esc>", "<Esc>"), [])).toEqual({ problem: "same" });
		expect(vimMappingProblem(mapping("<Leader>w", ":w<CR>"), [])).toEqual({
			problem: "unknownKey",
			key: "<Leader>",
		});
		expect(vimMappingProblem(mapping("Y", "y$<Bogus>"), [])).toEqual({
			problem: "unknownKey",
			key: "<Bogus>",
		});
		// The same keys in another mode are another mapping.
		const earlier = [mapping("<c-s>", ":w<CR>")];
		expect(vimMappingProblem(mapping("<C-s>", ":wq<CR>"), earlier)).toEqual({
			problem: "duplicate",
		});
		expect(vimMappingProblem(mapping("<C-s>", "<Esc>", "insert"), earlier)).toBeUndefined();
	});

	it("applies mappings in the editor's notation, and none that cannot be", () => {
		expect(
			appliedVimMapping({ ...mapping(" jk ", "<esc>", "insert"), recursive: true }, []),
		).toEqual({ mode: "insert", lhs: "jk", rhs: "<Esc>", recursive: true });
		expect(appliedVimMapping(mapping("j", "gj", "insert"), [])).toBeNull();
	});
});
