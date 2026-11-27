import { defineConfig } from "orval";

// Generates the API client from crates/ahab/openapi.json: request functions,
// TanStack Query hooks and MSW mock handlers, one folder per API tag, which
// is one per server component (`platform.health` → `platform-health/`).
export default defineConfig({
	ahab: {
		input: {
			target: "../../crates/ahab/openapi.json",
		},
		output: {
			mode: "tags-split",
			target: "src/shared/api/generated/ahab.ts",
			client: "react-query",
			httpClient: "fetch",
			clean: true,
			indexFiles: false,
			// Mock responses use the examples defined on the server's types.
			mock: { generators: [{ type: "msw", useExamples: true }] },
			override: {
				mutator: { path: "src/shared/api/fetch.ts", name: "apiFetch" },
				fetch: { includeHttpResponseReturnType: false },
			},
		},
	},
});
