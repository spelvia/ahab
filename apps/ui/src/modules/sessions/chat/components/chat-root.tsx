import { createContext, useContext } from "react";

import { relativeTo } from "@/modules/sessions/chat/model";

/** The chat's project folder, so its paths can show relative to it. */
export const ChatRoot = createContext<string | undefined>(undefined);

/** A path as the chat shows it: inside its project, relative to it. */
export function useShownPath(): (path: string) => string {
	const root = useContext(ChatRoot);
	return (path) => relativeTo(root, path);
}

export const ChatFileActions = createContext<{
	project?: string;
	diff?: (file: string) => Promise<void>;
}>({});
