import { cn } from "cn";
import { type RefObject, useEffect, useState } from "react";

import { revealMessage } from "@/modules/sessions/chat/hooks";
import type { RailTurn } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";

/**
 * Down the conversation's left edge: a mark for each turn (each of your
 * messages), the whole chat's, loaded or not. The one in view is brighter;
 * clicking one scrolls to it, loading the older history it is in first,
 * and hovering shows what you asked.
 */
export function TurnRail({
	chatId,
	turns,
	scroller,
}: {
	chatId: string;
	turns: RailTurn[];
	/** The conversation's scrolling element, whose rows carry `data-seq`. */
	scroller: RefObject<HTMLElement | null>;
}) {
	const { t } = useTranslation("sessions");
	const [current, setCurrent] = useState<number>();
	useEffect(() => {
		const view = scroller.current;
		if (!view || !turns.length) return;
		// The loaded turn whose message was last scrolled past the top third.
		const update = () => {
			const top = view.getBoundingClientRect().top + view.clientHeight / 3;
			let found: number | undefined;
			for (const turn of turns) {
				const row = view.querySelector(`[data-seq="${turn.seq}"]`);
				if (!row) continue;
				if (found === undefined || row.getBoundingClientRect().top <= top) found = turn.seq;
			}
			setCurrent(found);
		};
		update();
		view.addEventListener("scroll", update, { passive: true });
		return () => view.removeEventListener("scroll", update);
	}, [turns, scroller]);
	if (!turns.length) return null;
	const go = (seq: number) => {
		const row = scroller.current?.querySelector(`[data-seq="${seq}"]`);
		if (row) row.scrollIntoView({ block: "start", behavior: "auto" });
		else revealMessage(chatId, seq);
	};
	return (
		<nav
			aria-label={t("turnRail.label")}
			className="group/rail hover:bg-popover focus-within:bg-popover absolute top-1/2 left-1 z-10 flex max-h-[70%] max-w-[80%] -translate-y-1/2 [scrollbar-width:none] flex-col overflow-y-auto rounded-md py-1.5 focus-within:shadow-md hover:shadow-md max-md:hidden"
		>
			{turns.map((turn, index) => (
				<button
					key={turn.seq}
					type="button"
					onClick={() => go(turn.seq)}
					title={turn.text.slice(0, 120)}
					aria-label={t("turnRail.turnLabel", { number: index + 1, text: turn.text.slice(0, 60) })}
					aria-current={turn.seq === current ? "true" : undefined}
					className="group/turn focus-visible:ring-ring hover:bg-accent flex h-2 shrink-0 items-center gap-1 rounded-sm px-1.5 text-left outline-none group-focus-within/rail:h-6 group-hover/rail:h-6 focus-visible:ring-1 focus-visible:ring-inset"
				>
					<span
						className={cn(
							"h-0.5 shrink-0 rounded-full transition-colors",
							turn.seq === current
								? "bg-foreground w-4"
								: "bg-muted-foreground/70 group-hover/turn:bg-foreground w-2.5",
						)}
					/>
					<span className="text-popover-foreground hidden max-w-52 truncate pr-1 text-xs group-focus-within/rail:block group-hover/rail:block">
						{turn.text.split("\n")[0]}
					</span>
				</button>
			))}
		</nav>
	);
}
