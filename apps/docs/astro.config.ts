import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

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
			],
		}),
	],
});
