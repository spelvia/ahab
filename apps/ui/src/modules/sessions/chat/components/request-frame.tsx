import { cn } from "cn";
import { type KeyboardEvent, type ReactNode, useId } from "react";

import { useAnswerRequest } from "@/modules/sessions/chat/api";
import type {
	AgentRequest,
	AnswerRequest,
	RequestOutcome,
	ToolCallItem,
} from "@/modules/sessions/chat/model";
import { chatActionFor } from "@/shared/keymap/model";
import { useKeymap } from "@/shared/keymap/use-keymap";

/** What every request component is given. */
export interface RequestProps {
	chatId: string;
	request: AgentRequest;
	/** It waits for an answer now; else it shows how it ended. */
	pending: boolean;
	outcome?: RequestOutcome;
	/** The agent's name: "Claude Code". */
	agent: string;
	/** The tool call it is about, when the conversation has it. */
	call?: ToolCallItem;
}

/** Sends an answer to one request. */
export function useAnswer(chatId: string, requestId: string) {
	const mutation = useAnswerRequest(chatId);
	const send = (data: AnswerRequest) => mutation.mutate({ id: chatId, request: requestId, data });
	return { send, pending: mutation.isPending, error: mutation.error };
}

/** Whether a key went to something being typed in, where Enter and Esc are the field's. */
function typing(target: EventTarget): boolean {
	if (!(target instanceof HTMLElement)) return false;
	if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
	if (target instanceof HTMLSelectElement) return true;
	return (
		target instanceof HTMLInputElement &&
		!["checkbox", "radio", "button", "submit", "reset"].includes(target.type)
	);
}

/**
 * A request's card: amber while it waits for you (the one amber place),
 * quiet once answered. Focusable, so the chat can move to it; while it has
 * the focus, the chat's keys (Enter, Esc by default) answer it.
 */
export function RequestFrame({
	request,
	pending,
	icon,
	title,
	meta,
	onAllow,
	onDeny,
	questionLayout = false,
	shortcuts,
	children,
}: {
	request: AgentRequest;
	pending: boolean;
	icon: ReactNode;
	title: ReactNode;
	/** A line beside the title: a step, a folder. */
	meta?: ReactNode;
	onAllow?: () => void;
	onDeny?: () => void;
	/** What the keys do here, said under the actions. */
	questionLayout?: boolean;
	shortcuts?: string;
	children: ReactNode;
}) {
	const map = useKeymap();
	const titleId = useId();
	const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		if (!pending || event.defaultPrevented || typing(event.target)) return;
		const action = chatActionFor(map, event.nativeEvent);
		const run = action === "chat.allow" ? onAllow : action === "chat.deny" ? onDeny : undefined;
		if (!run) return;
		// A focused button's own Enter presses it instead.
		if (action === "chat.allow" && event.target instanceof HTMLButtonElement) return;
		event.preventDefault();
		run();
	};
	return (
		<section
			aria-labelledby={titleId}
			data-request={request.id}
			data-pending={pending || undefined}
			tabIndex={-1}
			onKeyDown={onKeyDown}
			className={cn(
				"focus-visible:ring-ring/50 scroll-mt-16 scroll-mb-24 rounded-[10px] border outline-none focus-visible:ring-[3px]",
				questionLayout
					? "border-border bg-card p-4 sm:p-5"
					: pending
						? "border-warning/50 bg-warning/8 p-3.5 sm:p-4"
						: "border-border/70 px-3.5 py-2.5",
			)}
		>
			<header className={questionLayout ? "flex flex-col gap-1.5" : "flex items-start gap-2.5"}>
				{!questionLayout && (
					<span
						className={cn(
							"mt-0.5 shrink-0 [&_svg]:size-4",
							pending ? "text-warning" : "text-muted-foreground",
						)}
						aria-hidden
					>
						{icon}
					</span>
				)}
				<h3
					id={titleId}
					className={cn(
						"min-w-0 flex-1 text-sm font-semibold text-pretty break-words",
						pending ? "text-heading" : "text-foreground",
						questionLayout && "text-base leading-snug",
					)}
				>
					{title}
				</h3>
				{meta ? (
					<span
						className={cn(
							"text-muted-foreground shrink-0 pt-px text-xs tabular-nums",
							questionLayout && "order-first",
						)}
					>
						{meta}
					</span>
				) : null}
			</header>
			<div
				className={cn(
					"flex flex-col gap-3",
					!questionLayout && "sm:pl-6.5",
					pending ? "mt-2.5" : "mt-1",
				)}
			>
				{children}
			</div>
			{pending && shortcuts ? (
				<p className="text-muted-foreground mt-2 hidden pl-6.5 text-xs sm:block">{shortcuts}</p>
			) : null}
		</section>
	);
}

/** A card's answers: a row of buttons that wraps; on a phone each is a full touch target. */
export function Actions({ children }: { children: ReactNode }) {
	return <div className="flex flex-wrap items-center gap-2">{children}</div>;
}

/** Buttons in a card: 44px tall on touch screens, the small size elsewhere. */
export const touch = "h-11 px-4 sm:h-8 sm:px-3";

/** What was chosen, once answered. */
export function Outcome({ children }: { children: ReactNode }) {
	return <p className="text-muted-foreground text-sm">{children}</p>;
}

/** The server's refusal of an answer, said where it was given. */
export function AnswerError({ error }: { error: Error | null }) {
	if (!error) return null;
	return (
		<p role="alert" className="text-destructive text-sm">
			{error.message}
		</p>
	);
}

/** A text field in a card, in the app's field style. */
export const fieldClass =
	"bg-background border-input text-foreground placeholder:text-muted-foreground focus-visible:ring-ring/40 block w-full rounded-md border px-2.5 py-2 text-base outline-none focus-visible:ring-[3px] sm:text-sm";
