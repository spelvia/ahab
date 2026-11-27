import { bundledLanguages } from "@/shared/syntax/bundled";
import { contributedLanguages } from "@/shared/syntax/catalog";

/** A language as the LSPs settings need it: its name and its default tools, in order. */
export interface ToolLanguage {
	id: string;
	name: string;
	tools: string[];
}

/** Ahab's languages and plugins', a plugin's replacing a bundled one of the same ID. */
export function toolLanguages(): ToolLanguage[] {
	const contributed = contributedLanguages();
	const replaced = new Set(contributed.map((item) => item.id));
	return [...bundledLanguages.filter((item) => !replaced.has(item.id)), ...contributed]
		.filter((item) => !item.declaration.hidden)
		.map((item) => ({
			id: item.id,
			name: item.name,
			tools: Array.isArray(item.declaration.tools)
				? item.declaration.tools.filter((tool): tool is string => typeof tool === "string")
				: [],
		}));
}

/** Names languages by their ID; an ID no language has stays as it is. */
export function languageNamer(): (id: string) => string {
	const names = new Map(toolLanguages().map((item) => [item.id, item.name]));
	return (id) => names.get(id) ?? id;
}
