import { copyFile, readFile } from "node:fs/promises";

import starlight from "@astrojs/starlight";
import type { AstroIntegration } from "astro";
import { defineConfig } from "astro/config";

/**
 * The install script at /install.sh (`curl -fsSL https://useahab.com/install.sh | sh`),
 * from scripts/install.sh: copied into the build, served in development.
 * It still downloads Ahab from GitHub's releases.
 */
const INSTALL_SCRIPT = new URL("../../scripts/install.sh", import.meta.url);
function installScript(): AstroIntegration {
	return {
		name: "ahab-install-script",
		hooks: {
			"astro:server:setup": ({ server }) => {
				server.middlewares.use("/install.sh", async (_request, response) => {
					response.setHeader("content-type", "text/x-shellscript; charset=utf-8");
					response.end(await readFile(INSTALL_SCRIPT));
				});
			},
			"astro:build:done": async ({ dir }) => {
				await copyFile(INSTALL_SCRIPT, new URL("install.sh", dir));
			},
		},
	};
}

export default defineConfig({
	site: "https://useahab.com",
	// The landing page is at /, the documentation under /docs (its pages are
	// in src/content/docs/docs/), so links between pages are `/docs/editor/`.
	// Next to the other development ports (see README.md); AHAB_DEV_HOSTS
	// lists other names it answers to, such as a reverse proxy's.
	server: {
		port: 5184,
		host: "127.0.0.1",
		allowedHosts: (process.env.AHAB_DEV_HOSTS ?? "")
			.split(",")
			.map((host) => host.trim())
			.filter(Boolean),
	},
	integrations: [
		installScript(),
		starlight({
			title: "Ahab",
			description:
				"A minimal, self-hosted agentic workspace you can configure and extend with plugins.",
			logo: {
				light: "./src/assets/ahab-mark.svg",
				dark: "./src/assets/ahab-mark-white.svg",
			},
			favicon: "/favicon.svg",
			lastUpdated: false,
			customCss: ["./src/styles/custom.css"],
			sidebar: [
				{
					label: "Getting Started",
					items: [
						{ label: "Introduction", slug: "docs" },
						{
							label: "Installation",
							items: [
								{ label: "Overview", slug: "docs/installation" },
								{ label: "CLI", slug: "docs/installation/cli" },
								{ label: "From source", slug: "docs/installation/from-source" },
							],
						},
						{ label: "Configuration", slug: "docs/configuration" },
						{ label: "Editor", slug: "docs/editor" },
						{ label: "LSPs and Problems", slug: "docs/language-tools" },
						{ label: "Security", slug: "docs/security" },
					],
				},
				{
					label: "Plugin SDK",
					items: [
						{ label: "Overview", slug: "docs/plugins" },
						{ label: "Quick start", slug: "docs/plugins/quick-start" },
						{ label: "Manifest reference", slug: "docs/plugins/manifest" },
						{ label: "Web entry", slug: "docs/plugins/web-entry" },
						{ label: "Daemon entry", slug: "docs/plugins/daemon-entry" },
						{ label: "Settings, secrets and storage", slug: "docs/plugins/settings" },
						{ label: "Versioning and compatibility", slug: "docs/plugins/versioning" },
						{ label: "SDK reference", slug: "docs/plugins/reference" },
					],
				},
				{
					label: "Development",
					items: [
						{ label: "Compatibility", slug: "docs/development/compatibility" },
						{ label: "Tool registry", slug: "docs/development/tool-registry" },
					],
				},
				{ label: "Changelog", link: "/changelog/" },
			],
		}),
	],
});
