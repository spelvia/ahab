import { describe, expect, it } from "vitest";

import { fileIconId, folderIconId, type IconTheme } from "@/shared/file-icons/theme";

const theme: IconTheme = {
	iconDefinitions: {},
	file: "file",
	folder: "folder",
	folderExpanded: "folder-open",
	fileNames: { "cargo.toml": "cargo", dockerfile: "docker" },
	fileExtensions: { "test.tsx": "test-jsx", tsx: "react_ts", "d.ts": "typescript-def" },
	languageIds: { typescript: "typescript", rust: "rust" },
	folderNames: { src: "folder-src", node_modules: "folder-node" },
	folderNamesExpanded: { src: "folder-src-open" },
	light: { fileExtensions: { tsx: "react_ts_light" } },
};

describe("file icon themes", () => {
	it("finds a file's icon by name, then longest extension, then language", () => {
		expect(fileIconId(theme, "crates/ahab/Cargo.toml")).toBe("cargo");
		expect(fileIconId(theme, "docker/Dockerfile")).toBe("docker");
		expect(fileIconId(theme, "ui/src/view.test.tsx")).toBe("test-jsx");
		expect(fileIconId(theme, "ui/src/view.tsx")).toBe("react_ts");
		expect(fileIconId(theme, "types/index.d.ts")).toBe("typescript-def");
		expect(fileIconId(theme, "src/model.ts")).toBe("typescript");
		expect(fileIconId(theme, "src/main.rs")).toBe("rust");
		expect(fileIconId(theme, "notes.unknown")).toBe("file");
		expect(fileIconId(theme, "ui/src/view.tsx", true)).toBe("react_ts_light");
	});

	it("finds a folder's icon, open or closed", () => {
		expect(folderIconId(theme, "ui/src", false)).toBe("folder-src");
		expect(folderIconId(theme, "ui/src", true)).toBe("folder-src-open");
		expect(folderIconId(theme, "node_modules", true)).toBe("folder-open");
		expect(folderIconId(theme, "docs", false)).toBe("folder");
	});
});
