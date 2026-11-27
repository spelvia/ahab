import { Check, Copy, GitFork } from "lucide-react";
import { type ReactNode, useState } from "react";

import { workedFor } from "@/modules/sessions/chat/model";
import { formatDate, useTranslation } from "@/shared/i18n";
import { copyText } from "@/shared/ui/clipboard";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

/**
 * Under a finished turn: copy the agent's answer, fork the chat from here,
 * and how long it worked.
 */
export function TurnFooter({
	answer,
	durationMs,
	endedAt,
	forkOptions,
}: {
	/** The agent's answer in the turn, as Markdown. */
	answer: string;
	durationMs?: number;
	/** When it ended (ms since the epoch). */
	endedAt?: number;
	/** Fork's choices from this turn, in a popover; absent where the agent cannot. */
	forkOptions?: (close: () => void) => ReactNode;
}) {
	const { t } = useTranslation("sessions");
	const [copied, setCopied] = useState(false);
	const [forking, setForking] = useState(false);
	const button =
		"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-7 place-items-center rounded-md outline-none focus-visible:ring-2";
	return (
		<div className="text-muted-foreground -ml-1.5 flex flex-wrap items-center gap-0.5 text-xs">
			{answer ? (
				<button
					type="button"
					className={button}
					aria-label={copied ? t("turnFooter.copied") : t("turnFooter.copy")}
					title={copied ? t("turnFooter.copied") : t("turnFooter.copy")}
					onClick={() =>
						void copyText(answer).then((done) => {
							if (!done) return;
							setCopied(true);
							setTimeout(() => setCopied(false), 1500);
						})
					}
				>
					{copied ? <Check className="text-success size-3.5" /> : <Copy className="size-3.5" />}
				</button>
			) : null}
			{forkOptions ? (
				<Popover open={forking} onOpenChange={setForking}>
					<PopoverTrigger
						className={button}
						aria-label={t("turnFooter.fork")}
						title={t("turnFooter.fork")}
					>
						<GitFork className="size-3.5" />
					</PopoverTrigger>
					<PopoverContent align="start" side="top" className="w-56 p-1">
						{forkOptions(() => setForking(false))}
					</PopoverContent>
				</Popover>
			) : null}
			{durationMs !== undefined ? (
				<span className="ml-1.5 tabular-nums">
					{t("turnFooter.worked", { time: workedFor(durationMs) })}
				</span>
			) : null}
			{endedAt !== undefined ? <EndedAt at={endedAt} /> : null}
		</div>
	);
}

/** When a turn ended: its time today, else its day and time; in full on hover. */
function EndedAt({ at }: { at: number }) {
	const { t } = useTranslation("sessions");
	// Read once: a turn's footer does not change its words at midnight.
	const [today] = useState(() => new Date().toDateString() === new Date(at).toDateString());
	const short = formatDate(
		at,
		today
			? { hour: "numeric", minute: "2-digit" }
			: { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" },
	);
	const full = formatDate(at, { dateStyle: "full", timeStyle: "medium" });
	return (
		<time
			dateTime={new Date(at).toISOString()}
			title={t("turnFooter.ended", { time: full })}
			className="before:text-muted-foreground/60 tabular-nums before:mx-1.5 before:content-['·']"
		>
			{short}
		</time>
	);
}
