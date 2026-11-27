import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { SettingsForm } from "@/modules/plugins/setting/components/settings-form";
import { mockServer } from "@/testing/mock-server";
import { renderWithQuery } from "@/testing/render";

const schema = {
	type: "object",
	properties: {
		greeting: { type: "string", default: "hello", description: "What it says." },
		loud: { type: "boolean", default: false },
	},
};

describe("SettingsForm", () => {
	it("shows each field with what it inherits, and saves what is set here", async () => {
		const saved: unknown[] = [];
		mockServer.use(
			http.get("*/api/v1/plugins/echo/settings", () =>
				HttpResponse.json({ schema, values: { greeting: "hi" } }),
			),
			http.get("*/api/v1/plugins/echo/settings/effective", () =>
				HttpResponse.json({ values: { greeting: "hi", loud: false } }),
			),
			http.put("*/api/v1/plugins/echo/settings", async ({ request }) => {
				saved.push({ url: new URL(request.url).search, body: await request.json() });
				return HttpResponse.json({ schema, values: {} });
			}),
			http.get("*/api/v1/machines", () => HttpResponse.json({ items: [] })),
			http.get("*/api/v1/projects", () => HttpResponse.json({ items: [] })),
		);
		renderWithQuery(<SettingsForm plugin="echo" />);
		const greeting = await screen.findByRole("textbox", { name: "Greeting" });
		expect(greeting).toHaveValue("hi");
		expect(screen.getByText("Set here")).toBeInTheDocument();
		expect(screen.getByText("What it says.")).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Save changes" })).not.toBeInTheDocument();

		await userEvent.clear(greeting);
		await userEvent.type(greeting, "hey");
		await userEvent.click(screen.getByRole("switch", { name: "Loud" }));
		await userEvent.click(screen.getByRole("button", { name: "Save changes" }));
		await screen.findByRole("textbox", { name: "Greeting" });
		expect(saved).toEqual([
			{ url: "?scope=server", body: { values: { greeting: "hey", loud: true } } },
		]);
	});

	it("resets a field to inherit again", async () => {
		const saved: unknown[] = [];
		mockServer.use(
			http.get("*/api/v1/plugins/echo/settings", () =>
				HttpResponse.json({ schema, values: { greeting: "hi" } }),
			),
			http.get("*/api/v1/plugins/echo/settings/effective", () => HttpResponse.json({ values: {} })),
			http.put("*/api/v1/plugins/echo/settings", async ({ request }) => {
				saved.push(await request.json());
				return HttpResponse.json({ schema, values: {} });
			}),
			http.get("*/api/v1/machines", () => HttpResponse.json({ items: [] })),
			http.get("*/api/v1/projects", () => HttpResponse.json({ items: [] })),
		);
		renderWithQuery(<SettingsForm plugin="echo" />);
		await screen.findByDisplayValue("hi");
		await userEvent.click(screen.getByRole("button", { name: "Reset" }));
		expect(screen.getByRole("textbox", { name: "Greeting" })).toHaveAttribute(
			"placeholder",
			"hello",
		);
		await userEvent.click(screen.getByRole("button", { name: "Save changes" }));
		expect(saved).toEqual([{ values: {} }]);
	});
});
