import type { ComponentType } from "react";

import type { ContextReference } from "@/shared/chat-context/model";

/** What a component shown after a chat's finished turn is given. */
export interface AfterTurnProps {
	chatId: string;
	/** The turn's ID: its `turnStart` event's `turnId`. */
	turnId: string;
	projectId?: string;
	/** Puts context in this chat's composer; none when it cannot be written to. */
	onAddContext?: (references: ContextReference[]) => void;
	onOpenFile?: (path: string, line?: number) => void;
}

// Components a module adds after each finished turn of a chat (such as the
// problems a turn left); registered once, at startup, so chats
// need not know the modules that add them.
const components: ComponentType<AfterTurnProps>[] = [];

/** Shows `component` after each finished turn of every chat. */
export function registerAfterTurn(component: ComponentType<AfterTurnProps>) {
	if (!components.includes(component)) components.push(component);
}

/** What the registered components show after a finished turn. */
export function AfterTurn(props: AfterTurnProps) {
	return (
		<>
			{components.map((Component, index) => (
				// The list only grows, at startup.
				// oxlint-disable-next-line no-array-index-key
				<Component key={index} {...props} />
			))}
		</>
	);
}
