import { describe, expect, it } from "vitest";

import { absolutePath, fileTooltip, sessionTooltip } from "@/app/shell/tab-tooltip";
import type { Session } from "@/shared/api/generated/ahab.schemas";

const atlas = { path: "/mnt/data/ahab", machineName: "atlas" };

describe("tab tooltips", () => {
	it("joins the project's folder and the file's path", () => {
		expect(absolutePath("/mnt/data/ahab", "crates/ahab/src/main.rs")).toBe(
			"/mnt/data/ahab/crates/ahab/src/main.rs",
		);
		expect(absolutePath("/mnt/data/ahab/", "/a.rs")).toBe("/mnt/data/ahab/a.rs");
		expect(absolutePath("/", "etc/hosts")).toBe("/etc/hosts");
		expect(absolutePath("C:\\code\\app", "src/main.rs")).toBe("C:\\code\\app\\src\\main.rs");
		expect(absolutePath("/w", "")).toBe("/w");
	});

	it("shows a file's path on its machine", () => {
		expect(fileTooltip("crates/ahab/src/main.rs", atlas)).toBe(
			"/mnt/data/ahab/crates/ahab/src/main.rs — atlas",
		);
		expect(fileTooltip("src/a.rs")).toBe("src/a.rs");
		expect(fileTooltip("external:abc", atlas, "vec.rs")).toBe(
			"vec.rs (outside the project) — atlas",
		);
	});

	it("shows a session's title and agent, a terminal's command and folder", () => {
		const chat = {
			kind: "chat",
			title: "Fix the build",
			chats: [{ agent: "claude" }],
			terminal: null,
		} as unknown as Session;
		expect(sessionTooltip(chat, atlas)).toBe("Fix the build · claude — atlas");
		const terminal = {
			kind: "terminal",
			title: "Terminal",
			chats: [],
			terminal: { command: "make dev", title: null },
		} as unknown as Session;
		expect(sessionTooltip(terminal, atlas)).toBe("make dev in /mnt/data/ahab — atlas");
		const shell = {
			...terminal,
			terminal: { command: null, title: "alex@atlas: ~/ahab" },
		} as unknown as Session;
		expect(sessionTooltip(shell)).toBe("Shell (alex@atlas: ~/ahab)");
	});
});
