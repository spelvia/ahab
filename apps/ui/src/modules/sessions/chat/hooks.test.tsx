import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { PropsWithChildren } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { STOP_WAIT_MS, useSideWorkStops } from "@/modules/sessions/chat/hooks";
import { stopOutcome } from "@/modules/sessions/chat/model";
import { mockServer } from "@/testing/mock-server";

afterEach(() => vi.useRealTimers());

function mount(chat: string, statuses: Record<string, string>) {
	const query = new QueryClient();
	const wrapper = ({ children }: PropsWithChildren) => (
		<QueryClientProvider client={query}>{children}</QueryClientProvider>
	);
	return renderHook(
		({ now }: { now: Record<string, string> }) => useSideWorkStops(chat, (id) => now[id]),
		{ wrapper, initialProps: { now: statuses } },
	);
}

function receipts(
	chat: string,
	answers: Record<string, { disposition: string; message?: string }>,
) {
	mockServer.use(
		http.post(`*/api/v1/chats/${chat}/stop-work`, async ({ request }) => {
			const { id } = (await request.json()) as { id: string };
			return HttpResponse.json(answers[id]);
		}),
	);
}

it("reads every provider answer to a stop", () => {
	expect(stopOutcome("requested")).toBe("stopping");
	expect(stopOutcome("accepted")).toBe("stopping");
	expect(stopOutcome("unconfirmed")).toBe("stopping");
	expect(stopOutcome("confirmed")).toBe("stopped");
	expect(stopOutcome("alreadyFinished")).toBe("finished");
	expect(stopOutcome("notRunning")).toBe("finished");
	expect(stopOutcome("rejected")).toBe("rejected");
	expect(stopOutcome("somethingNewer")).toBe("stopping");
});

it("keeps each accepted stop stopping until its agent reports it, separately", async () => {
	const chat = "stops";
	receipts(chat, {
		a: { disposition: "accepted" },
		b: { disposition: "rejected", message: "It is busy" },
		c: { disposition: "unconfirmed" },
	});
	const statuses = { a: "running", b: "running", c: "running" };
	const hook = mount(chat, statuses);
	await act(async () => {
		hook.result.current.stop("agent", "a", "Explorer");
		hook.result.current.stop("task", "b", "npm test");
		hook.result.current.stop("task", "c", "cargo build");
	});
	await vi.waitFor(() => expect(hook.result.current.stops.b?.phase).toBe("failed"));
	expect(hook.result.current.stops).toEqual({
		a: { phase: "stopping", unconfirmed: false },
		b: { phase: "failed", message: "It is busy" },
		c: { phase: "stopping", unconfirmed: true },
	});
	// The agent reports a stopped: only it settles.
	hook.rerender({ now: { ...statuses, a: "stopped" } });
	expect(hook.result.current.stops.a).toBeUndefined();
	expect(hook.result.current.stops.c?.phase).toBe("stopping");
});

it("gives up waiting after a while, so it can be stopped again", async () => {
	vi.useFakeTimers({ shouldAdvanceTime: true });
	const chat = "slow-stop";
	receipts(chat, { a: { disposition: "accepted" } });
	const hook = mount(chat, { a: "running" });
	await act(async () => hook.result.current.stop("agent", "a", "Explorer"));
	await vi.waitFor(() =>
		expect(hook.result.current.stops.a).toEqual({ phase: "stopping", unconfirmed: false }),
	);
	act(() => vi.advanceTimersByTime(STOP_WAIT_MS));
	expect(hook.result.current.stops.a).toEqual({ phase: "timedOut" });
});
