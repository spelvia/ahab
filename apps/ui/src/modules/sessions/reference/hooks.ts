import { useSyncExternalStore } from "react";

import type { ContextReference } from "@/shared/chat-context/model";
import {
	chatDrafts,
	chatTarget,
	rememberChatDrafts,
	rememberChatTarget,
} from "@/shared/preferences/store";

/** Two references to the same thing. */
function same(a: ContextReference, b: ContextReference): boolean {
	return (
		a.kind === b.kind &&
		a.path === b.path &&
		a.startLine === b.startLine &&
		a.endLine === b.endLine &&
		a.text === b.text
	);
}

// Each composer's context not sent yet, by key: a session's ID, or
// `new:<window>` for a New session window. Kept in this browser until sent,
// shared by every window that shows the same composer.

type Drafts = Record<string, ContextReference[]>;

const EMPTY: ContextReference[] = [];
let drafts: Drafts | undefined;
const listeners = new Set<() => void>();

function current(): Drafts {
	drafts ??= chatDrafts<ContextReference>();
	return drafts;
}

function set(next: Drafts) {
	drafts = next;
	rememberChatDrafts(next);
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** Changing composers' context drafts. */
export const draftStore = {
	/** Adds references to a composer, leaving out ones it holds already. */
	add(key: string, references: ContextReference[]) {
		const list = current()[key] ?? [];
		const added = references.filter((item) => !list.some((held) => same(held, item)));
		set({ ...current(), [key]: [...list, ...added] });
		return added;
	},
	remove(key: string, ids: string[]) {
		const list = current()[key] ?? [];
		set({ ...current(), [key]: list.filter((item) => !ids.includes(item.id)) });
	},
	clear(key: string) {
		const { [key]: _gone, ...rest } = current();
		set(rest);
	},
	/** Moves a New session window's context to the session it started. */
	move(from: string, to: string) {
		const list = current()[from];
		if (!list?.length) return;
		const { [from]: _gone, ...rest } = current();
		set({ ...rest, [to]: [...(rest[to] ?? []), ...list] });
	},
	get(key: string): ContextReference[] {
		return current()[key] ?? EMPTY;
	},
};

/** A composer's context draft, kept up to date. */
export function useDraft(key: string | undefined): ContextReference[] {
	return useSyncExternalStore(
		subscribe,
		() => (key ? (current()[key] ?? EMPTY) : EMPTY),
		() => EMPTY,
	);
}

// The chat context goes to: the session window used last in a project.

const targetListeners = new Set<() => void>();

function subscribeTarget(listener: () => void) {
	targetListeners.add(listener);
	return () => targetListeners.delete(listener);
}

/** Marks a session as used (typed in, focused, opened) in its project. */
export function touchChatTarget(projectId: string, sessionId: string) {
	if (chatTarget(projectId) === sessionId) return;
	rememberChatTarget(projectId, sessionId);
	for (const listener of targetListeners) listener();
}

/** The session context goes to in a project. */
export function useChatTargetId(projectId: string | undefined): string | undefined {
	return useSyncExternalStore(
		subscribeTarget,
		() => (projectId ? chatTarget(projectId) : undefined),
		() => undefined,
	);
}
