import { ChevronDown, MessageSquarePlus } from "lucide-react";

import { useChatContext } from "@/shared/chat-context/context";
import type { ContextReference } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { mac } from "@/shared/platform";

/**
 * Add to "<target>" with an arrow for the chooser: the target is
 * the session window used last. `references` is asked for only on click.
 */
export function AddToChat({
	references,
	compact,
	className,
}: {
	references: () => ContextReference[];
	/** An icon and the arrow only, for tight places. */
	compact?: boolean;
	className?: string;
}) {
	const { t } = useTranslation();
	const chat = useChatContext();
	if (!chat) return null;
	const key = mac ? "⌘L" : "Ctrl+L";
	const label = chat.target
		? t("chatContext.addTo", { title: chat.target.title })
		: t("chatContext.addToChat");
	return (
		<span
			className={`bg-popover text-popover-foreground border-border inline-flex items-stretch overflow-hidden rounded-md border text-xs shadow-sm ${className ?? ""}`}
		>
			<button
				type="button"
				title={chat.target ? t("withKey", { label, key }) : t("chatContext.chooseForThis")}
				aria-label={label}
				onMouseDown={(event) => event.preventDefault()}
				onClick={() => {
					const list = references();
					if (list.length) chat.add(list);
				}}
				className="hover:bg-accent focus-visible:ring-ring flex max-w-64 items-center gap-1.5 px-2 py-1 outline-none focus-visible:ring-2 focus-visible:ring-inset"
			>
				<MessageSquarePlus className="size-3.5 shrink-0" />
				{compact ? null : <span className="truncate">{label}</span>}
			</button>
			<button
				type="button"
				title={t("withKey", {
					label: t("chatContext.choose"),
					key: mac ? "⇧⌘L" : "Ctrl+Shift+L",
				})}
				aria-label={t("chatContext.choose")}
				onMouseDown={(event) => event.preventDefault()}
				onClick={() => {
					const list = references();
					if (list.length) chat.choose(list);
				}}
				className="hover:bg-accent focus-visible:ring-ring border-border border-l px-1 outline-none focus-visible:ring-2 focus-visible:ring-inset"
			>
				<ChevronDown className="size-3.5" />
			</button>
		</span>
	);
}
