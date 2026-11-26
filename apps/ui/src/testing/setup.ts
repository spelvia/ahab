import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterAll, afterEach, beforeAll } from "vitest";

import { mockServer } from "@/testing/mock-server";

// jsdom has no ResizeObserver, which Radix components use to measure
// themselves.
class ResizeObserverStub implements ResizeObserver {
	observe() {}
	unobserve() {}
	disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub;

beforeAll(() => mockServer.listen({ onUnhandledFrame: "error" }));
afterEach(() => {
	cleanup();
	mockServer.resetHandlers();
});
afterAll(() => mockServer.close());
