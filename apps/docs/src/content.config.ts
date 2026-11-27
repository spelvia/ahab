import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";
import { glob } from "astro/loaders";
import { defineCollection } from "astro:content";

export const collections = {
	docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
	// The changelog: each release's notes, as the GitHub Release shows them
	// (docs/releases/<version>.md), named by version.
	releases: defineCollection({
		loader: glob({
			pattern: "[0-9]*.md",
			base: "../../docs/releases",
			generateId: ({ entry }) => entry.replace(/\.md$/, ""),
		}),
	}),
};
