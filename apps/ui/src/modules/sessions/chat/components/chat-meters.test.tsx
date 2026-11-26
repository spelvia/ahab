import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";

import { SessionUsageMeter } from "@/modules/sessions/chat/components/chat-meters";
import { changeAppearance } from "@/shared/appearance";

const original = window.matchMedia;
afterEach(() => {
	window.matchMedia = original;
	changeAppearance({ showCost: true, showTokens: true });
});

function phone(on: boolean) {
	window.matchMedia = vi.fn<(query: string) => MediaQueryList>().mockImplementation(
		(query) =>
			({
				matches: on && query.includes("max-width"),
				media: query,
				addEventListener: () => {},
				removeEventListener: () => {},
			}) as unknown as MediaQueryList,
	);
}

const context = { used: 50_000, window: 200_000 };
const spend = { tokens: 123_456, costUsd: 0.4218 };

it("shows the context, the tokens spent and the session cost in Session usage", async () => {
	phone(false);
	render(<SessionUsageMeter context={context} spend={spend} />);
	const trigger = screen.getByRole("button", { name: /^Session usage: context 25% full/ });
	expect(trigger).toHaveAccessibleName(
		"Session usage: context 25% full, 123k tokens spent, $0.4218",
	);
	expect(trigger).not.toHaveTextContent("$");
	await userEvent.click(trigger);
	const panel = screen.getByRole("dialog", { name: "Session usage" });
	expect(panel).toHaveTextContent("50,000 of 200,000 tokens");
	expect(panel).toHaveTextContent("Tokens spent this session123,456");
	expect(panel).toHaveTextContent("Session cost$0.4218");
});

it("hides the cost and the tokens spent as Settings → Agent editor says", async () => {
	phone(false);
	changeAppearance({ showCost: false, showTokens: false });
	render(<SessionUsageMeter context={context} spend={spend} />);
	const trigger = screen.getByRole("button", { name: "Session usage: context 25% full" });
	expect(trigger).not.toHaveTextContent("$");
	await userEvent.click(trigger);
	const panel = screen.getByRole("dialog", { name: "Session usage" });
	expect(panel).not.toHaveTextContent("Session cost");
	expect(panel).not.toHaveTextContent("Tokens spent");
	expect(panel).toHaveTextContent("50,000 of 200,000 tokens");
});

it("opens as a sheet on phones", async () => {
	phone(true);
	changeAppearance({ showCost: true, showTokens: false });
	render(<SessionUsageMeter context={undefined} spend={spend} />);
	await userEvent.click(screen.getByRole("button", { name: "Session usage: $0.4218" }));
	const sheet = screen.getByRole("dialog", { name: "Session usage" });
	expect(sheet).toHaveTextContent("Session cost$0.4218");
	expect(sheet).not.toHaveTextContent("Tokens spent");
});

it("shows nothing before there is anything to show", () => {
	phone(false);
	const { container } = render(<SessionUsageMeter context={undefined} spend={{}} />);
	expect(container).toBeEmptyDOMElement();
});
