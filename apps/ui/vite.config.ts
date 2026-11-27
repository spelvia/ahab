import { fileURLToPath, URL } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { loadEnv } from "vite";
import { defineConfig } from "vitest/config";

/**
 * Pages Vite serves in development, which count as the server's own: on
 * this computer, or at a name in `AHAB_DEV_HOSTS` (such as a reverse proxy).
 */
const ownOrigin = (origin: unknown, hosts: string[]) => {
	if (typeof origin !== "string") return false;
	const match = /^https?:\/\/([^/:]+)(:\d+)?$/.exec(origin);
	return Boolean(match?.[1] && ["127.0.0.1", "localhost", ...hosts].includes(match[1]));
};

export default defineConfig(({ mode }) => {
	// The server's settings live in the repository's .env, next to this
	// package; only AHAB_* values are read.
	const env = loadEnv(mode, fileURLToPath(new URL("../..", import.meta.url)), "AHAB_");
	const apiAddr = env.AHAB_LISTEN_ADDR ?? "127.0.0.1:8180";
	// Other names this dev server answers to, comma-separated.
	const devHosts = (env.AHAB_DEV_HOSTS ?? "")
		.split(",")
		.map((host) => host.trim())
		.filter(Boolean);

	return {
		plugins: [tanstackRouter({ target: "react", autoCodeSplitting: true }), react(), tailwindcss()],
		resolve: {
			alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
		},
		build: {
			// File icons stay files of their own, fetched when shown, rather
			// than all inlined into the icon theme's code; so do the bundled
			// languages' grammars and queries, fetched when needed.
			assetsInlineLimit: (file: string) =>
				file.includes("material-icon-theme") || file.includes("/languages/") ? false : undefined,
		},
		server: {
			host: "127.0.0.1",
			port: Number(env.AHAB_DEV_PORT ?? 5183),
			strictPort: true,
			allowedHosts: devHosts,
			proxy: {
				// changeOrigin: the server answers only to the names it knows,
				// and this one is Vite's, not the server's.
				// The server takes changes only from its own origin:
				// pages Vite serves count as the server's, other sites do not.
				"/api": {
					target: `http://${apiAddr}`,
					changeOrigin: true,
					configure: (proxy) => {
						proxy.on("proxyReq", (request) => {
							if (ownOrigin(request.getHeader("origin"), devHosts))
								request.setHeader("origin", `http://${apiAddr}`);
						});
					},
				},
			},
		},
		test: {
			environment: "jsdom",
			include: ["src/**/*.test.{ts,tsx}"],
			setupFiles: ["src/testing/setup.ts"],
			maxWorkers: "50%",
		},
	};
});
