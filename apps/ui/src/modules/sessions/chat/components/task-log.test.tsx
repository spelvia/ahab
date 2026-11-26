import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { readTaskOutput } from "@/modules/sessions/chat/api";
import { filterLines, TaskLog } from "@/modules/sessions/chat/components/task-log";
import { logRendering, stripAnsi } from "@/modules/sessions/chat/components/terminal-output";

vi.mock("@/modules/sessions/chat/api", () => ({
	readTaskOutput: vi.fn<typeof readTaskOutput>(),
}));

describe("logRendering", () => {
	it("keeps text without escapes plain", () => {
		expect(logRendering("a\nb")).toEqual({ mode: "plain", text: "a\nb" });
	});

	it("colours only the tail of a long log, from a line start", () => {
		const text = "\u001b[31mred\u001b[0m old\n" + "x".repeat(20) + "\nnew \u001b[32mok\u001b[0m";
		expect(logRendering(text, 15)).toEqual({
			mode: "ansi",
			head: "red old\n" + "x".repeat(20) + "\n",
			tail: "new \u001b[32mok\u001b[0m",
		});
		expect(logRendering("\u001b[1mx", 15)).toEqual({ mode: "ansi", head: "", tail: "\u001b[1mx" });
	});

	it("strips colour, cursor and title escapes", () => {
		expect(stripAnsi("\u001b[1;31ma\u001b[0m\u001b[2K\u001b]0;title\u0007b")).toBe("ab");
	});
});

describe("filterLines", () => {
	it("keeps matching lines, case-insensitively", () => {
		expect(filterLines("Error one\nok\nerror two", "ERROR")).toBe("Error one\nerror two");
		expect(filterLines("a\nb", "")).toBe("a\nb");
		expect(filterLines("a\nb", "z")).toBeNull();
	});
});

describe("TaskLog", () => {
	it("reads ahead a bounded amount and goes on when asked", async () => {
		const read = vi.mocked(readTaskOutput);
		let page = 0;
		read.mockImplementation(async () => {
			page++;
			return {
				text: "y".repeat(65_536),
				eof: false,
				nextCursor: String(page),
				availability: "complete",
			} as never;
		});
		render(
			<TaskLog
				chatId="c"
				task={{ id: "t", kind: { type: "command", command: "make" }, status: "completed" }}
				onClose={() => {}}
			/>,
		);
		const more = await screen.findByRole("button", { name: "Read more" });
		expect(read).toHaveBeenCalledTimes(8);
		await userEvent.click(more);
		await screen.findByRole("button", { name: "Read more" });
		await waitFor(() => expect(read).toHaveBeenCalledTimes(16));
	});

	it("shows the whole command behind its one-line title", async () => {
		vi.mocked(readTaskOutput).mockResolvedValue({
			text: "",
			eof: true,
			availability: "complete",
		} as never);
		const command = "/bin/zsh -lc 'set -e\ncd app\npnpm test --filter ui'";
		render(
			<TaskLog
				chatId="c"
				task={{ id: "t", kind: { type: "command", command }, status: "running" }}
				onClose={() => {}}
			/>,
		);
		expect(screen.getByRole("heading")).toHaveTextContent("cd app …");
		await userEvent.click(screen.getByText("Show the full command"));
		const full = screen.getByRole("region", { name: "Command" });
		expect(full).toHaveTextContent("set -e");
		expect(full).toHaveTextContent("pnpm test --filter ui");
		expect(full).not.toHaveTextContent("zsh -lc");
	});
});
