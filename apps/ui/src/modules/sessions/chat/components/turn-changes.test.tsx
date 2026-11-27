import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";

import { TurnChanges } from "@/modules/sessions/chat/components/turn-changes";
import { addTurnChange, diffStat, type TurnChange } from "@/modules/sessions/chat/model";

it("counts a diff's lines added and removed, not its headers", () => {
	expect(diffStat("--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-a\n+b\n+c\n d\n")).toEqual({
		added: 2,
		removed: 1,
	});
});

it("keeps one row per file: created then edited stays created, lines summed", () => {
	const changes = new Map<string, TurnChange>();
	addTurnChange(changes, { path: "a.ts", kind: "add", diff: "+1\n+2\n" });
	addTurnChange(changes, { path: "a.ts", kind: "update", diff: "-1\n+x\n" });
	addTurnChange(changes, { path: "b.ts", kind: "update", diff: "+y\n" });
	addTurnChange(changes, { path: "b.ts", kind: "delete" });
	expect([...changes.values()]).toEqual([
		{ path: "a.ts", kind: "add", exists: true, added: 3, removed: 1 },
		{ path: "b.ts", kind: "delete", exists: false, added: 1, removed: 0 },
	]);
});

const change = (path: string, kind: TurnChange["kind"] = "update"): TurnChange => ({
	path,
	kind,
	exists: kind !== "delete",
	added: 2,
	removed: 1,
});

it("says how many files changed and their lines, each file with what happened", () => {
	render(<TurnChanges changes={[change("src/server.ts"), change("src/rate-limit.ts", "add")]} />);
	expect(screen.getByRole("button", { name: /Changed 2 files/ })).toHaveAttribute(
		"aria-expanded",
		"true",
	);
	expect(screen.getByLabelText("4 lines added, 2 removed")).toBeInTheDocument();
	expect(screen.getByText("rate-limit.ts")).toBeInTheDocument();
	expect(screen.getByText("Added")).toBeInTheDocument();
	expect(screen.getByText("Edited")).toBeInTheDocument();
});

it("folds many files, and shows the first few until asked for the rest", async () => {
	const user = userEvent.setup();
	const many = Array.from({ length: 9 }, (_, i) => change(`src/file-${i}.ts`));
	render(<TurnChanges changes={many} />);
	const summary = screen.getByRole("button", { name: /Changed 9 files/ });
	expect(summary).toHaveAttribute("aria-expanded", "false");
	expect(screen.queryByText("file-0.ts")).toBeNull();
	await user.click(summary);
	expect(screen.getByText("file-5.ts")).toBeInTheDocument();
	expect(screen.queryByText("file-6.ts")).toBeNull();
	await user.click(screen.getByRole("button", { name: "Show 3 more" }));
	expect(screen.getByText("file-8.ts")).toBeInTheDocument();
});
