import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, it, vi } from "vitest";

import { Composer } from "@/modules/sessions/chat/components/composer";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

it("keeps the draft on a rejected send and clears it after acceptance", async () => {
	const send = vi
		.fn<() => Promise<void>>()
		.mockRejectedValueOnce(new Error("Disconnected"))
		.mockResolvedValueOnce(undefined);
	renderWithQuery(<Composer placeholder="Draft" onSend={send} />);
	const box = screen.getByRole("textbox", { name: "Message" });
	await userEvent.type(box, "A draft{Enter}");
	expect(send).toHaveBeenCalledTimes(1);
	expect(box).toHaveValue("A draft");
	await userEvent.type(box, "{Enter}");
	expect(send).toHaveBeenCalledTimes(2);
	expect(box).toHaveValue("");
});

it("restores typed text when the workbench reopens", async () => {
	const send = vi.fn<() => Promise<void>>().mockResolvedValue(undefined);
	const composer = <Composer messageDraftKey="restore-draft" placeholder="Draft" onSend={send} />;
	const first = renderWithQuery(composer);
	await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "Kept draft");
	first.unmount();
	renderWithQuery(composer);
	const box = screen.getByRole("textbox", { name: "Message" });
	expect(box).toHaveValue("Kept draft");
	await userEvent.type(box, "{Enter}");
	expect(box).toHaveValue("");
});

it("selects an explicit session skill and waits for send", async () => {
	const send = vi.fn<() => void>();
	const read = vi
		.fn<NonNullable<React.ComponentProps<typeof Composer>["readCommands"]>>()
		.mockResolvedValue([
			{ kind: "skill", id: "repo/review", name: "review", description: "Review changes" },
		]);
	renderWithQuery(
		<Composer commandChat="chat" readCommands={read} placeholder="Draft" onSend={send} />,
	);
	const box = screen.getByRole("textbox", { name: "Message" });
	await userEvent.type(box, "/rev");
	await userEvent.click(await screen.findByRole("button", { name: /Review changes/ }));
	expect(send).not.toHaveBeenCalled();
	await userEvent.type(box, "staged files{Enter}");
	expect(send).toHaveBeenCalledWith("/review staged files", [], [], undefined, {
		kind: "skill",
		id: "repo/review",
	});
});

it("sends the selected server and preserves named MCP arguments containing spaces", async () => {
	const send = vi.fn<() => void>();
	const servers = ["first", "second"];
	const read: NonNullable<React.ComponentProps<typeof Composer>["readCommands"]> = async () =>
		servers.map((server) => ({
			kind: "mcpPrompt",
			name: "audit",
			id: server,
			server,
			description: server + " audit",
			arguments: [{ name: "issue", required: true }],
		}));
	renderWithQuery(
		<Composer commandChat="mcp" readCommands={read} placeholder="Draft" onSend={send} />,
	);
	const box = screen.getByRole("textbox", { name: "Message" });
	await userEvent.type(box, "/audit");
	await userEvent.click(await screen.findByRole("button", { name: /second audit/ }));
	await userEvent.type(box, "{Enter}");
	expect(send).not.toHaveBeenCalled();
	await userEvent.type(screen.getByRole("textbox", { name: /issue/ }), "two words");
	await userEvent.type(box, "{Enter}");
	expect(send).toHaveBeenCalledWith("/audit", [], [], undefined, {
		kind: "mcpPrompt",
		id: "second",
		arguments: { issue: "two words" },
	});
});

it("attaches a project file at the cursor and preserves the rest of the draft", async () => {
	mockServer.use(
		http.get("*/api/v1/projects/project/files/index", () =>
			HttpResponse.json({ paths: ["src/main.rs", "tests/main.rs"], truncated: false }),
		),
	);
	const send = vi.fn<() => void>();
	renderWithQuery(
		<Composer projectId="project" draftKey="mentions-test" placeholder="Draft" onSend={send} />,
	);
	const box = screen.getByRole("textbox", { name: "Message" }) as HTMLTextAreaElement;
	await userEvent.type(box, "Check  tomorrow");
	box.setSelectionRange(6, 6);
	await userEvent.keyboard("@main");
	await userEvent.click(await screen.findByRole("button", { name: "src/main.rs" }));
	expect(box).toHaveValue("Check  tomorrow");
	await userEvent.keyboard("{Enter}");
	expect(send).toHaveBeenCalledWith(
		"Check  tomorrow",
		[],
		[expect.objectContaining({ kind: "file", path: "src/main.rs", projectId: "project" })],
		undefined,
	);
});

it("stops the turn on Escape, unless the agent waits for an answer", async () => {
	const stop = vi.fn<() => void>();
	const view = renderWithQuery(
		<Composer placeholder="Draft" working onStop={stop} onSend={vi.fn<() => void>()} />,
	);
	const box = screen.getByRole("textbox", { name: "Message" });
	await userEvent.type(box, "{Escape}");
	expect(stop).toHaveBeenCalledTimes(1);
	view.unmount();
	renderWithQuery(
		<Composer
			placeholder="Draft"
			working
			awaitingAnswer
			onStop={stop}
			onSend={vi.fn<() => void>()}
		/>,
	);
	await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "{Escape}");
	expect(stop).toHaveBeenCalledTimes(1);
	// The Stop button still stops it.
	await userEvent.click(screen.getByRole("button", { name: "Stop" }));
	expect(stop).toHaveBeenCalledTimes(2);
});

it("puts the agent's settings in one sheet on phones, and keeps the meters in the toolbar", async () => {
	const original = window.matchMedia;
	window.matchMedia = vi.fn<(query: string) => MediaQueryList>().mockImplementation(
		(query) =>
			({
				matches: query.includes("max-width"),
				media: query,
				addEventListener: () => {},
				removeEventListener: () => {},
			}) as unknown as MediaQueryList,
	);
	try {
		renderWithQuery(
			<Composer
				placeholder="Draft"
				onSend={vi.fn<() => void>()}
				settingsSummary="Opus · High"
				settings={<button type="button">Model picker</button>}
				meters={<button type="button">Usage meter</button>}
			/>,
		);
		expect(screen.queryByRole("button", { name: "Model picker" })).not.toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Usage meter" })).toBeVisible();
		await userEvent.click(screen.getByRole("button", { name: "Agent settings: Opus · High" }));
		const sheet = screen.getByRole("dialog", { name: "Agent settings" });
		expect(sheet).toContainElement(screen.getByRole("button", { name: "Model picker" }));
	} finally {
		window.matchMedia = original;
	}
});

it("shows the agent's settings in the toolbar on wider screens", () => {
	renderWithQuery(
		<Composer
			placeholder="Draft"
			onSend={vi.fn<() => void>()}
			settings={<button type="button">Model picker</button>}
		/>,
	);
	expect(screen.getByRole("button", { name: "Model picker" })).toBeInTheDocument();
	expect(screen.queryByRole("button", { name: /Agent settings/ })).not.toBeInTheDocument();
});
