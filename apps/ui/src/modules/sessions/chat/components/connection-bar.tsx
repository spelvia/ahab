import { useEffect, useState } from "react";

import type { Link } from "@/modules/sessions/chat/stream";
import { useTranslation } from "@/shared/i18n";

/**
 * Across the top of a chat while its live stream is not open: a moving bar
 * and "Connecting…" or "Reconnecting…". It waits a moment first, so a quick
 * connection never flashes it.
 */
export function ConnectionBar({ link, reconnect }: { link: Link; reconnect?: () => void }) {
	const { t } = useTranslation("sessions");
	// The state that has lasted a moment; open is taken at once.
	const [settled, setSettled] = useState<Link>("open");
	useEffect(() => {
		const timer = setTimeout(() => setSettled(link), link === "open" ? 0 : 600);
		return () => clearTimeout(timer);
	}, [link]);
	if (link === "open" || settled !== link) return null;
	return (
		<div role="status" className="pointer-events-none absolute inset-x-0 top-0 z-20">
			<div className="bg-primary/15 h-0.5 overflow-hidden">
				<div className="loading-bar bg-primary h-full w-1/3" />
			</div>
			<p className="bg-popover text-muted-foreground border-border mx-auto mt-2 w-fit rounded-md border px-2 py-1 text-xs shadow-sm">
				{t(link === "reconnecting" ? "chatView.reconnecting" : "chatView.connecting")}
				{link === "reconnecting" && reconnect && (
					<button type="button" onClick={reconnect} className="pointer-events-auto ml-2 underline">
						{t("tools.retry")}
					</button>
				)}
			</p>
		</div>
	);
}
