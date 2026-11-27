import manifest from "material-icon-theme/dist/material-icons.json";

import type { IconTheme } from "@/shared/file-icons/theme";

// Each icon is its own small file, fetched the first time it shows.
const urls = import.meta.glob<string>("../../../node_modules/material-icon-theme/icons/*.svg", {
	query: "?url",
	import: "default",
	eager: true,
});

const byName = new Map(
	Object.entries(urls).map(([path, url]) => [path.slice(path.lastIndexOf("/") + 1), url]),
);

/** Material Icon Theme (MIT), Ahab's bundled file icons. */
export const theme = manifest as IconTheme;

/** An icon's address, from its definition's `iconPath`. */
export function iconUrl(id: string): string | undefined {
	const path = theme.iconDefinitions[id]?.iconPath;
	return path ? byName.get(path.slice(path.lastIndexOf("/") + 1)) : undefined;
}
