import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { KeyboardPage } from "@/modules/platform/keyboard/pages/keyboard-page";
import { keymap, resetShortcuts } from "@/shared/keymap/store";
import { currentVimMappings, setVimMappings } from "@/shared/keymap/vim";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

beforeEach(() => {
	mockServer.use(http.get("*/api/v1/features", () => HttpResponse.json({ items: [] })));
});
afterEach(() => {
	resetShortcuts();
	setVimMappings([]);
});

async function openAction(title: string) {
	renderWithRouter(<KeyboardPage />);
	await userEvent.click(await screen.findByRole("button", { name: new RegExp(`^${title}`) }));
	return within(screen.getByRole("button", { name: new RegExp(`^${title}`) }).closest("li")!);
}

describe("KeyboardPage", () => {
	it("adds a key by pressing it, removes it, and resets", async () => {
		const row = await openAction("Split the window");
		await userEvent.click(row.getByRole("button", { name: "Add a key" }));
		fireEvent.keyDown(window, { code: "ShiftLeft", shiftKey: true });
		expect(row.getByRole("status")).toHaveTextContent("Shift");
		fireEvent.keyDown(window, { code: "KeyJ", altKey: true, shiftKey: true });
		expect(keymap()["window.split"]).toEqual(["Alt+Backslash", "Alt+Shift+KeyJ"]);
		expect(row.getByText("Changed")).toBeInTheDocument();

		await userEvent.click(
			row.getByRole("button", { name: "Remove Alt+Shift+J from Split the window" }),
		);
		expect(keymap()["window.split"]).toEqual(["Alt+Backslash"]);
		await userEvent.click(row.getByRole("button", { name: /Reset to Ahab's keys/ }));
		expect(row.queryByText("Changed")).not.toBeInTheDocument();
	});

	it("asks before taking a key from another action, and refuses bare keys", async () => {
		const row = await openAction("Split the window");
		await userEvent.click(row.getByRole("button", { name: "Add a key" }));
		fireEvent.keyDown(window, { code: "KeyJ" });
		expect(row.getByRole("alert")).toHaveTextContent("would fire while you type");
		await userEvent.click(row.getByRole("button", { name: "Try another" }));
		fireEvent.keyDown(window, { code: "KeyK", ctrlKey: true });
		expect(row.getByRole("alert")).toHaveTextContent("Command palette");
		expect(keymap().palette).toEqual(["Ctrl+KeyK"]);
		await userEvent.click(row.getByRole("button", { name: "Move it here" }));
		expect(keymap().palette).toEqual([]);
		expect(keymap()["window.split"]).toContain("Ctrl+KeyK");
	});

	it("finds a shortcut by its keys", async () => {
		renderWithRouter(<KeyboardPage />);
		await userEvent.click(
			await screen.findByRole("button", { name: "Find a shortcut by pressing its keys" }),
		);
		fireEvent.keyDown(window, { code: "KeyP", ctrlKey: true });
		expect(screen.getByRole("button", { name: /^Go to file/ })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: /^Command palette/ })).not.toBeInTheDocument();
		await userEvent.click(screen.getByRole("button", { name: "Clear the key filter" }));
		expect(screen.getByRole("button", { name: /^Command palette/ })).toBeInTheDocument();
	});

	it("lists the editor's commands with Ahab's, and changes their keys the same way", async () => {
		const row = await openAction("Delete the line");
		expect(screen.getByRole("heading", { name: "Editor" })).toBeInTheDocument();
		await userEvent.click(row.getByRole("button", { name: "Add a key" }));
		fireEvent.keyDown(window, { code: "KeyD", ctrlKey: true, shiftKey: true });
		expect(keymap()["editor.deleteLine"]).toEqual(["Ctrl+Shift+KeyK", "Ctrl+Shift+KeyD"]); // gitleaks:allow (keys, not a secret)
	});

	it("keeps Vim mappings, and says why one cannot be applied", async () => {
		renderWithRouter(<KeyboardPage />);
		await userEvent.click(await screen.findByRole("button", { name: "Add a mapping" }));
		await userEvent.selectOptions(
			screen.getByRole("combobox", { name: "Mapping 1: mode" }),
			"insert",
		);
		await userEvent.type(screen.getByRole("textbox", { name: "Mapping 1: keys typed" }), "j");
		await userEvent.type(
			screen.getByRole("textbox", { name: "Mapping 1: keys they stand for" }),
			"<Esc>",
		);
		expect(screen.getByRole("alert")).toHaveTextContent("could no longer be typed");
		await userEvent.type(screen.getByRole("textbox", { name: "Mapping 1: keys typed" }), "k");
		expect(screen.queryByRole("alert")).not.toBeInTheDocument();
		expect(currentVimMappings()).toEqual([{ mode: "insert", lhs: "jk", rhs: "<Esc>" }]);
		await userEvent.click(screen.getByRole("button", { name: "Remove mapping 1" }));
		expect(currentVimMappings()).toEqual([]);
	});
});
