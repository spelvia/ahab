import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { ConnectionBar } from "@/modules/sessions/chat/components/connection-bar";

afterEach(() => vi.useRealTimers());

it("counts down to the next try, and shows Retry now being tried", () => {
	vi.useFakeTimers();
	const reconnect = vi.fn<() => void>();
	const view = render(
		<ConnectionBar link="reconnecting" retryAt={Date.now() + 4_000} reconnect={reconnect} />,
	);
	act(() => vi.advanceTimersByTime(1_000));
	expect(screen.getByRole("status").textContent).toContain("Trying again in 3 seconds");
	fireEvent.click(screen.getByRole("button", { name: "Retry now" }));
	expect(reconnect).toHaveBeenCalledOnce();
	expect(screen.getByRole("status").textContent).toContain("Reconnecting…");
	expect(screen.queryByRole("button")).toBeNull();
	// The try failed at once: waiting again, with a new time.
	view.rerender(
		<ConnectionBar link="reconnecting" retryAt={Date.now() + 2_000} reconnect={reconnect} />,
	);
	act(() => vi.advanceTimersByTime(900));
	expect(screen.getByRole("status").textContent).toContain("Trying again in 2 seconds");
});
