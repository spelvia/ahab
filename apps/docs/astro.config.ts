import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

/**
 * Where the site is published: https://useahab.com/docs. Links between
 * pages include it (`/docs/editor/`).
 */
const BASE = "/docs";

export default defineConfig({
	site: "https://useahab.com",
	base: BASE,
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
						{ label: "Introduction", slug: "" },
						{
							label: "Installation",
							items: [
								{ label: "Overview", slug: "installation" },
								{ label: "CLI", slug: "installation/cli" },
								{ label: "From source", slug: "installation/from-source" },
							],
						},
						{ label: "Configuration", slug: "configuration" },
						{ label: "Editor", slug: "editor" },
						{ label: "LSPs and Problems", slug: "language-tools" },
						{ label: "Security", slug: "security" },
					],
				},
				{
					label: "Plugin SDK",
					items: [
						{ label: "Overview", slug: "plugins" },
						{ label: "Quick start", slug: "plugins/quick-start" },
						{ label: "Manifest reference", slug: "plugins/manifest" },
						{ label: "Web entry", slug: "plugins/web-entry" },
						{ label: "Daemon entry", slug: "plugins/daemon-entry" },
						{ label: "Settings, secrets and storage", slug: "plugins/settings" },
						{ label: "Versioning and compatibility", slug: "plugins/versioning" },
						{ label: "SDK reference", slug: "plugins/reference" },
					],
				},
				{
					label: "Development",
					items: [
						{ label: "Compatibility", slug: "development/compatibility" },
						{ label: "Tool registry", slug: "development/tool-registry" },
					],
				},
			],
		}),
	],
});
