import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";

import { moved, QueueList } from "@/modules/sessions/chat/components/queue-list";
import type { QueuedMessage } from "@/modules/sessions/chat/model";

const message = (id: string, text: string, more: Partial<QueuedMessage> = {}): QueuedMessage => ({
	id,
	text,
	attachments: 0,
	interrupt: false,
	unconfirmed: false,
	...more,
});
const queue = [
	message("a", "First"),
	message("b", "Second\nwith more lines"),
	message("c", "Third"),
];

it("moves a message to another's place, or one step", () => {
	expect(moved(["a", "b", "c"], "c", "a")).toEqual(["c", "a", "b"]);
	expect(moved(["a", "b", "c"], "a", 1)).toEqual(["b", "a", "c"]);
	expect(moved(["a", "b", "c"], "a", -1)).toEqual(["a", "b", "c"]);
});

it("reorders waiting messages from the keyboard, the interrupting one staying first", async () => {
	const reorder = vi.fn<(order: string[]) => void>();
	render(
		<QueueList queue={[message("x", "Now", { interrupt: true }), ...queue]} onReorder={reorder} />,
	);
	const first = screen.getByRole("button", { name: "First" });
	first.focus();
	await userEvent.keyboard("{Alt>}{ArrowDown}{/Alt}");
	expect(reorder).toHaveBeenCalledWith(["b", "a", "c"]);
	const texts = screen.getAllByRole("listitem").map((item) => item.textContent);
	expect(texts[0]).toContain("Now");
	expect(texts[1]).toContain("Second");
});

it("expands a message to read all of it, and edits it in place", async () => {
	const edit = vi.fn<(id: string, text: string) => Promise<unknown>>(() => Promise.resolve());
	render(<QueueList queue={queue} onEdit={edit} />);
	const second = screen.getByRole("button", { name: /Second/ });
	expect(second).toHaveAttribute("aria-expanded", "false");
	await userEvent.click(second);
	expect(second).toHaveAttribute("aria-expanded", "true");

	const third = screen.getAllByRole("button", { name: "Edit this message" }).at(-1);
	if (!third) throw new Error("no Edit button");
	await userEvent.click(third);
	const field = screen.getByRole("textbox", { name: "Queued message" });
	expect(field).toHaveValue("Third");
	await userEvent.clear(field);
	await userEvent.type(field, "Third, changed{Control>}{Enter}{/Control}");
	expect(edit).toHaveBeenCalledWith("c", "Third, changed");
	expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
});
