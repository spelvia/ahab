import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { PropsWithChildren } from "react";
import { expect, it } from "vitest";

import { useProjectLayout } from "@/modules/projects/layout/api";
import { pendingProjectLayout } from "@/shared/preferences/store";
import { mockServer } from "@/testing/mock-server";

it("keeps changes across a remount and serializes overlapping layout saves", async () => {
	const project = "save-overlap";
	const url = `*/api/v1/projects/${project}/layouts/desktop`;
	const updatedAt = "2026-11-24T00:00:00Z";
	let unblock: (() => void) | undefined;
	const held = new Promise<void>((resolve) => {
		unblock = resolve;
	});
	const requests: Array<{ layout: object; revision: number }> = [];
	mockServer.use(
		http.get(url, () => HttpResponse.json({ layout: { tiles: 1 }, revision: 1, updatedAt })),
		http.put(url, async ({ request }) => {
			const body = (await request.json()) as (typeof requests)[number];
			requests.push(body);
			if (requests.length === 1) await held;
			return HttpResponse.json({ ...body, revision: body.revision + 1, updatedAt });
		}),
	);
	const client = new QueryClient();
	const wrapper = ({ children }: PropsWithChildren) => (
		<QueryClientProvider client={client}>{children}</QueryClientProvider>
	);
	const mount = () => renderHook(() => useProjectLayout(project, "desktop"), { wrapper });
	const first = mount();
	await waitFor(() => expect(first.result.current.layout?.revision).toBe(1));
	first.result.current.stage?.({ tiles: 2 }, 1);
	const firstSave = first.result.current.save({ tiles: 2 }, 1);
	await waitFor(() => expect(requests).toHaveLength(1));
	first.unmount();
	const reopened = mount();
	expect(reopened.result.current.layout).toEqual({ layout: { tiles: 2 }, revision: 1 });
	reopened.result.current.stage?.({ tiles: 3 }, 1);
	const secondSave = reopened.result.current.save({ tiles: 3 }, 1);
	expect(pendingProjectLayout(`project:${project}:desktop`)?.layout).toEqual({ tiles: 3 });
	expect(requests).toHaveLength(1);
	await act(async () => {
		unblock?.();
		await Promise.all([firstSave, secondSave]);
	});
	await waitFor(() => expect(reopened.result.current.layout?.revision).toBe(3));
	expect(requests).toEqual([
		{ layout: { tiles: 2 }, revision: 1 },
		{ layout: { tiles: 3 }, revision: 2 },
	]);
	expect(reopened.result.current.layout?.layout).toEqual({ tiles: 3 });
	expect(pendingProjectLayout(`project:${project}:desktop`)).toBeUndefined();
});
