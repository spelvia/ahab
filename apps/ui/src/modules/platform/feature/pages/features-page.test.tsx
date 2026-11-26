import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { FeaturesPage } from "@/modules/platform/feature/pages/features-page";
import { mockServer } from "@/testing/mock-server";
import { renderWithRouter } from "@/testing/render";

const feature = (id: string, name: string, required = false, enabled = true) => ({
	id,
	name,
	description: `${name} things.`,
	enabled,
	required,
});

describe("FeaturesPage", () => {
	it("switches a feature off, and keeps Sessions on", async () => {
		let items = [feature("sessions", "Sessions", true), feature("git", "Git")];
		const changes: unknown[] = [];
		mockServer.use(
			http.get("*/api/v1/features", () => HttpResponse.json({ items })),
			http.put("*/api/v1/features/git", async ({ request }) => {
				const body = (await request.json()) as { enabled: boolean };
				changes.push(body);
				items = [items[0]!, { ...items[1]!, enabled: body.enabled }];
				return HttpResponse.json({ items });
			}),
		);
		renderWithRouter(<FeaturesPage />);
		expect(await screen.findByText("Always on")).toBeInTheDocument();
		const git = screen.getByRole("switch", { name: "Git on" });
		await userEvent.click(git);
		expect(changes).toEqual([{ enabled: false }]);
		expect(await screen.findByRole("switch", { name: "Git off" })).toBeInTheDocument();
		expect(screen.queryByRole("switch", { name: /Sessions/ })).not.toBeInTheDocument();
	});
});
