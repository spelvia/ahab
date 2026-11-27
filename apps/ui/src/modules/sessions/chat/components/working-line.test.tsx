import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import { WorkingLine } from "@/modules/sessions/chat/components/working-line";
import type { Entry } from "@/modules/sessions/chat/model";

it("shows the running tool call without announcing it", () => {
	const entries: Entry[] = [
		{
			key: "item:t",
			kind: "item",
			done: false,
			streamed: "",
			item: {
				type: "toolCall",
				id: "t",
				status: "inProgress",
				detail: { kind: "command", command: "zsh -lc 'cargo test'" },
			},
		},
	];
	render(<WorkingLine agent="Codex" entries={entries} />);
	expect(screen.getByText("cargo test")).toBeInTheDocument();
	expect(screen.queryByRole("status")).toBeNull();
	expect(document.querySelector("[aria-live]")).toBeNull();
});

it("says what it waits for when no tool runs, such as a message being sent", () => {
	render(<WorkingLine agent="Claude Code" label="Sending to Claude Code…" />);
	expect(screen.getByText("Sending to Claude Code…")).toBeInTheDocument();
});
