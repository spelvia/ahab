import { afterEach, expect, it, vi } from "vitest";

import { watchFile } from "@/modules/projects/files/stream";

afterEach(() => {
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

it("resubscribes after a rejected watch and delivers the fresh snapshot", () => {
	vi.useFakeTimers();
	const streams: EventTarget[] = [];
	vi.stubGlobal(
		"EventSource",
		class extends EventTarget {
			constructor() {
				super();
				streams.push(this);
			}
			close() {}
		},
	);
	const changed = vi.fn<(version: string | null) => void>();
	const stop = watchFile("watch-recovery", "main.ts", changed);
	vi.advanceTimersByTime(150);
	const tell = (i: number, data: object) =>
		streams[i]?.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(data) }));
	tell(0, { type: "unwatched" });
	vi.advanceTimersByTime(2_000);
	expect(streams).toHaveLength(2);
	tell(0, { type: "changed", path: "main.ts", version: "stale" });
	tell(1, { type: "watching", versions: { "main.ts": "current" } });
	expect(changed.mock.calls).toEqual([["current"]]);
	stop();
	vi.advanceTimersByTime(30_000);
	expect(streams).toHaveLength(2);
});
