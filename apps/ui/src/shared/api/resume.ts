/**
 * Live connections and coming back to the page. A phone drops a page's
 * connections while it is locked or in the background, and their retries
 * fail there, waiting longer each time; so when the page is shown again (or
 * the network is back), every connection reconnects at once instead of
 * waiting out its delay.
 */

/**
 * Called when the page is back. `stale` says a connection that still looks
 * open may be dead: the page was away longer than the server's keep-alive,
 * or the network changed.
 */
type Listener = (stale: boolean) => void;

/** Longer than the server's 15-second keep-alive. */
const STALE_AFTER = 20_000;

const listeners = new Set<Listener>();
let watching = false;
let hiddenAt: number | undefined;

function resumed(stale: boolean) {
	for (const listener of listeners) listener(stale);
}

function watch() {
	if (watching || typeof window === "undefined") return;
	watching = true;
	document.addEventListener("visibilitychange", () => {
		if (document.visibilityState === "hidden") {
			hiddenAt = Date.now();
			return;
		}
		const away = hiddenAt === undefined ? 0 : Date.now() - hiddenAt;
		hiddenAt = undefined;
		resumed(away > STALE_AFTER);
	});
	// Restored from the back-forward cache, with its connections closed.
	window.addEventListener("pageshow", (event) => {
		if (event.persisted) resumed(true);
	});
	window.addEventListener("online", () => resumed(true));
}

/** Calls `listener` whenever the page is shown again or back online; answers what stops it. */
export function onResume(listener: Listener): () => void {
	watch();
	listeners.add(listener);
	return () => void listeners.delete(listener);
}

/**
 * How long to wait before reconnecting after `failures` failed tries, with
 * some jitter: at most 5 seconds while the page is shown (someone is
 * looking at it), up to 30 seconds while it is hidden.
 */
export function retryDelay(failures: number): number {
	const hidden = typeof document !== "undefined" && document.visibilityState === "hidden";
	const delay = Math.min(hidden ? 30_000 : 5_000, 1_000 * 2 ** Math.min(failures, 10));
	return delay * (0.8 + 0.4 * Math.random());
}

/**
 * A server-sent event stream kept open: the browser reconnects it after a
 * drop by itself, and `create` makes it again when the page is back and
 * the browser has given up on it, or it may be dead.
 */
export function keepOpen(create: () => EventSource): { close: () => void } {
	let source = create();
	const stop = onResume((stale) => {
		if (!stale && source.readyState !== EventSource.CLOSED) return;
		source.close();
		source = create();
	});
	return {
		close: () => {
			stop();
			source.close();
		},
	};
}
