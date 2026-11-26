import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useGitWatch, useIsWatched } from "@/modules/projects/git/api";

/** A stand-in for the browser's EventSource that the test drives. */
class FakeSource {
	static last: FakeSource | undefined;
	readonly url: string;
	closed = false;
	private handlers = new Map<string, (event: { data?: string }) => void>();
	constructor(url: string) {
		this.url = url;
		FakeSource.last = this;
	}
	addEventListener(type: string, handler: (event: { data?: string }) => void) {
		this.handlers.set(type, handler);
	}
	close() {
		this.closed = true;
	}
	tell(frame: object) {
		this.handlers.get("message")?.({ data: JSON.stringify(frame) });
	}
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe("watching a checkout", () => {
	it("refreshes its Git after changes, says it is watched, and stops", () => {
		vi.stubGlobal("EventSource", FakeSource);
		const client = new QueryClient();
		const invalidate = vi.spyOn(client, "invalidateQueries");
		const wrapper = ({ children }: { children: ReactNode }) => (
			<QueryClientProvider client={client}>{children}</QueryClientProvider>
		);
		const { result, unmount } = renderHook(
			() => {
				useGitWatch("w1", "lib", true);
				return useIsWatched("w1", "lib");
			},
			{ wrapper },
		);
		const source = FakeSource.last!;
		expect(source.url).toBe("/api/v1/projects/w1/git/watch?repository=lib");
		expect(result.current).toBe(false);

		act(() => source.tell({ type: "watching" }));
		expect(result.current).toBe(true);
		invalidate.mockClear();
		act(() => source.tell({ type: "changed" }));
		expect(invalidate).toHaveBeenCalledTimes(1);
		const { predicate } = invalidate.mock.calls[0]![0] as unknown as {
			predicate: (query: { queryKey: unknown[] }) => boolean;
		};
		expect(predicate({ queryKey: ["/api/v1/projects/w1/git/status"] })).toBe(true);
		expect(predicate({ queryKey: ["/api/v1/projects/w1/git/log"] })).toBe(false);

		// It cannot be watched: polling takes over.
		act(() => source.tell({ type: "unwatched", reason: "too many folders" }));
		expect(result.current).toBe(false);
		expect(source.closed).toBe(true);
		unmount();
	});
});
