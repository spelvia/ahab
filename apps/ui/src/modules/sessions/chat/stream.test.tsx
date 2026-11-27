import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { useConversation } from "@/modules/sessions/chat/stream";

const { read, refresh } = vi.hoisted(() => ({
	read: vi.fn<(id: string, before: number, signal: AbortSignal) => Promise<unknown>>(),
	refresh: vi.fn<(id: string) => void>(),
}));

vi.mock("@/modules/sessions/chat/api", () => ({
	useReadChatHistory: () => read,
	useChatRefresh: () => refresh,
}));

class Stream extends EventTarget {
	static urls: string[] = [];

	constructor(url: string) {
		super();
		Stream.urls.push(url);
	}

	close() {}
}

afterEach(() => {
	read.mockReset();
	Stream.urls = [];
	vi.unstubAllGlobals();
});

function answer(seq: number, id: string, text: string, type = "itemEnd") {
	return { seq, createdAt: "", event: { type, item: { id, type: "agentMessage", text } } };
}

it("opens at the latest cursor and loads older page entries only when asked", async () => {
	vi.stubGlobal("EventSource", Stream);
	read
		.mockResolvedValueOnce({ items: [answer(12, "answer", "Latest")], historyBefore: 12 })
		.mockResolvedValueOnce({
			items: [answer(7, "old", "Earlier"), answer(8, "answer", "", "itemStart")],
			historyBefore: 0,
		});
	const chat = renderHook(() => useConversation("chat"));
	await waitFor(() => expect(Stream.urls).toEqual(["/api/v1/chats/chat/stream?after=12"]));
	expect(read).toHaveBeenCalledTimes(1);
	expect(chat.result.current.entries.map((entry) => entry.key)).toEqual(["item:answer"]);
	await act(async () => {
		await chat.result.current.loadOlder();
	});
	expect(read.mock.calls.map(([, before]) => before)).toEqual([0, 12]);
	expect(chat.result.current.entries.map((entry) => entry.key)).toEqual([
		"item:old",
		"item:answer",
	]);
	expect(chat.result.current.entries.at(-1)).toMatchObject({
		done: true,
		item: { text: "Latest" },
	});
	expect(chat.result.current.hasOlder).toBe(false);
});

it("retries a failed opening page instead of starting a full-history stream", async () => {
	vi.stubGlobal("EventSource", Stream);
	read
		.mockRejectedValueOnce(new Error("temporarily offline"))
		.mockResolvedValueOnce({ items: [answer(8, "new", "Back")], historyBefore: 8 });
	const chat = renderHook(() => useConversation("chat"));
	await waitFor(() => expect(chat.result.current.link).toBe("reconnecting"));
	expect(Stream.urls).toEqual([]);
	act(() => window.dispatchEvent(new Event("online")));
	await waitFor(() => expect(Stream.urls).toEqual(["/api/v1/chats/chat/stream?after=8"]));
	expect(read.mock.calls.map(([, before]) => before)).toEqual([0, 0]);
});
