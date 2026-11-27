import { setupServer } from "msw/node";

/** The mock API for tests. Each test adds the handlers it needs. */
export const mockServer = setupServer();
