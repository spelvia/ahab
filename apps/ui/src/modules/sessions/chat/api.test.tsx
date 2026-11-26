import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { PropsWithChildren } from "react";
import { afterEach, expect, it } from "vitest";

import { useSendMessage } from "@/modules/sessions/chat/api";
import type { SendMessage } from "@/shared/api/generated/ahab.schemas";
import { pendingChatMessage } from "@/shared/preferences/store";
import { mockServer } from "@/testing/mock-server";

afterEach(() => sessionStorage.clear());

function mount(chat: string) {
	const query = new QueryClient();
	const wrapper = ({ children }: PropsWithChildren) => (
		<QueryClientProvider client={query}>{children}</QueryClientProvider>
	);
	return renderHook(() => useSendMessage(chat, true), { wrapper });
}

it("retries a lost response after reopening the chat with the same ID and delivery", async () => {
	const chat = "recovered-send";
	const posted: SendMessage[] = [];
	mockServer.use(
		http.get(`*/api/v1/chats/${chat}`, () => HttpResponse.json({ messageSubmissions: true })),
		http.post(`*/api/v1/chats/${chat}/messages`, async ({ request }) => {
			posted.push((await request.json()) as SendMessage);
			return posted.length === 1 ? HttpResponse.error() : new HttpResponse(null, { status: 202 });
		}),
	);
	const message = { text: "Keep this draft", attachments: ["a-file"], delivery: "queue" } as const;
	const draft: SendMessage = { ...message, attachments: [...message.attachments] };
	const first = mount(chat);
	await act(async () => {
		await expect(first.result.current.mutateAsync(draft)).rejects.toMatchObject({ status: 0 });
	});
	first.unmount();
	const retry = mount(chat);
	expect(retry.result.current.recovered?.submissionId).toBeTruthy();
	await act(async () => {
		await retry.result.current.mutateAsync({ ...draft, delivery: undefined });
	});
	expect(posted).toHaveLength(2);
	expect(posted[1]).toEqual(posted[0]);
	expect(pendingChatMessage(chat)).toBeUndefined();
});

it("does not retry via a version that cannot verify submission IDs", async () => {
	const chat = "older-server-send";
	let posts = 0;
	mockServer.use(
		http.get(`*/api/v1/chats/${chat}`, () => HttpResponse.json({})),
		http.post(`*/api/v1/chats/${chat}/messages`, () => {
			posts++;
			return HttpResponse.error();
		}),
	);
	const hook = mount(chat);
	await act(async () => {
		await expect(hook.result.current.mutateAsync({ text: "Maybe sent" })).rejects.toMatchObject({
			status: 0,
		});
		await expect(hook.result.current.mutateAsync({ text: "Maybe sent" })).rejects.toMatchObject({
			status: 409,
		});
	});
	expect(posts).toBe(1);
});

it("shows a message as sending until its own echo arrives, by submission ID", async () => {
	const chat = "echoed-send";
	let posted: SendMessage | undefined;
	mockServer.use(
		http.post(`*/api/v1/chats/${chat}/messages`, async ({ request }) => {
			posted = (await request.json()) as SendMessage;
			return new HttpResponse(null, { status: 202 });
		}),
	);
	const hook = mount(chat);
	let sent: Promise<void> | undefined;
	act(() => {
		sent = hook.result.current.mutateAsync({
			text: "Summarise",
			command: { id: "p", kind: "mcpPrompt", arguments: { topic: "tests" } },
		});
	});
	await act(async () => {
		await sent;
	});
	const outgoing = hook.result.current.outgoing;
	expect(outgoing).toMatchObject({ text: "Summarise\ntopic = tests", sent: true });
	expect(outgoing?.submissionId).toBe(posted?.submissionId);
	// Another submission's echo leaves it showing.
	act(() => hook.result.current.settleOutgoing("someone-else"));
	expect(hook.result.current.outgoing).toBe(outgoing);
	act(() => hook.result.current.settleOutgoing(posted?.submissionId ?? ""));
	expect(hook.result.current.outgoing).toBeUndefined();
});

it("stops showing a message as sending when it fails", async () => {
	const chat = "failed-send";
	mockServer.use(
		http.post(`*/api/v1/chats/${chat}/messages`, () =>
			HttpResponse.json({ code: "chat_closed", title: "Closed" }, { status: 409 }),
		),
	);
	const hook = mount(chat);
	await act(async () => {
		await expect(hook.result.current.mutateAsync({ text: "Lost" })).rejects.toBeTruthy();
	});
	expect(hook.result.current.outgoing).toBeUndefined();
});
