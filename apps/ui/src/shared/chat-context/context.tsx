import { createContext, useContext } from "react";

import type { ContextReference } from "@/shared/chat-context/model";

/** The chat context goes to by default: the session window used last. */
interface ChatTarget {
	sessionId: string;
	title: string;
}

/** What places that offer context can do; provided by the workbench. */
export interface ChatContextValue {
	/** Where Add to chat goes; none before any session was used here. */
	target: ChatTarget | null;
	/** Adds to the target's composer (or, without one, opens the chooser). */
	add: (references: ContextReference[]) => void;
	/** Opens the chooser: which chat gets them, or a new session. */
	choose: (references: ContextReference[]) => void;
	/** Opens a New session window whose composer holds them. */
	startNew: (references: ContextReference[]) => void;
}

export const ChatContext = createContext<ChatContextValue | null>(null);

/** Adding context to a chat, where the workbench offers it. */
export function useChatContext(): ChatContextValue | null {
	return useContext(ChatContext);
}
