import { useEffect, useState } from "react";

import type { Link } from "@/modules/sessions/chat/stream";
import { useTranslation } from "@/shared/i18n";

/** "Reconnecting…" stays at least this long after Retry now, so the press shows. */
const PRESSED = 800;

/**
 * Across the top of a chat while its live stream is not open: a moving bar
 * and "Connecting…" or "Reconnecting…" while it tries; while it waits to
 * try again, how long, with Retry now. It waits a moment first, so a quick
 * connection never flashes it.
 */
export function ConnectionBar({
	link,
	retryAt,
	reconnect,
}: {
	link: Link;
	/** When it tries again, while it waits to. */
	retryAt?: number;
	reconnect?: () => void;
}) {
	const { t } = useTranslation("sessions");
	// The state that has lasted a moment; open is taken at once.
	const [settled, setSettled] = useState<Link>("open");
	useEffect(() => {
		const timer = setTimeout(() => setSettled(link), link === "open" ? 0 : 600);
		return () => clearTimeout(timer);
	}, [link]);
	// The time, every quarter second while it counts down.
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		if (retryAt === undefined) return;
		const update = () => setNow(Date.now());
		const first = setTimeout(update, 0);
		const timer = setInterval(update, 250);
		return () => {
			clearTimeout(first);
			clearInterval(timer);
		};
	}, [retryAt]);
	const [pressed, setPressed] = useState(false);
	useEffect(() => {
		if (!pressed) return;
		const timer = setTimeout(() => setPressed(false), PRESSED);
		return () => clearTimeout(timer);
	}, [pressed]);
	if (link === "open" || settled !== link) return null;
	const seconds = retryAt === undefined ? 0 : Math.ceil((retryAt - now) / 1000);
	const waiting = link === "reconnecting" && !pressed && seconds > 0;
	return (
		<div role="status" className="pointer-events-none absolute inset-x-0 top-0 z-20">
			<div className="bg-primary/15 h-0.5 overflow-hidden">
				{waiting ? null : <div className="loading-bar bg-primary h-full w-1/3" />}
			</div>
			<p className="bg-popover text-muted-foreground border-border mx-auto mt-2 w-fit rounded-md border px-2 py-1 text-xs shadow-sm">
				{waiting
					? t("chatView.waiting", { count: seconds })
					: t(link === "reconnecting" ? "chatView.reconnecting" : "chatView.connecting")}
				{waiting && reconnect && (
					<button
						type="button"
						onClick={() => {
							setPressed(true);
							reconnect();
						}}
						className="pointer-events-auto ml-2 underline"
					>
						{t("tools.retry")}
					</button>
				)}
			</p>
		</div>
	);
}
