/**
 * File icon themes in VS Code's format (`iconDefinitions`, `fileNames`,
 * `fileExtensions`, `languageIds`, `folderNames`, and their `light`
 * overrides), so any VS Code icon theme can be used: Material Icon Theme is
 * bundled, and plugins may bring others.
 */

interface IconAssociations {
	file?: string;
	folder?: string;
	folderExpanded?: string;
	rootFolder?: string;
	rootFolderExpanded?: string;
	fileNames?: Record<string, string>;
	fileExtensions?: Record<string, string>;
	languageIds?: Record<string, string>;
	folderNames?: Record<string, string>;
	folderNamesExpanded?: Record<string, string>;
	rootFolderNames?: Record<string, string>;
	rootFolderNamesExpanded?: Record<string, string>;
}

export interface IconTheme extends IconAssociations {
	iconDefinitions: Record<string, { iconPath?: string }>;
	/** Overrides for light colour schemes. */
	light?: IconAssociations;
}

/**
 * VS Code's own languages, by extension: themes name many files by their
 * language (`typescript`) rather than their extension.
 */
const LANGUAGES: Record<string, string> = {
	ts: "typescript",
	mts: "typescript",
	cts: "typescript",
	tsx: "typescriptreact",
	js: "javascript",
	mjs: "javascript",
	cjs: "javascript",
	jsx: "javascriptreact",
	json: "json",
	jsonc: "jsonc",
	md: "markdown",
	markdown: "markdown",
	py: "python",
	rs: "rust",
	go: "go",
	rb: "ruby",
	java: "java",
	c: "c",
	h: "c",
	cc: "cpp",
	cpp: "cpp",
	hpp: "cpp",
	cs: "csharp",
	fs: "fsharp",
	php: "php",
	swift: "swift",
	lua: "lua",
	sh: "shellscript",
	bash: "shellscript",
	zsh: "shellscript",
	ps1: "powershell",
	bat: "bat",
	cmd: "bat",
	html: "html",
	htm: "html",
	css: "css",
	scss: "scss",
	sass: "sass",
	less: "less",
	xml: "xml",
	svg: "svg",
	yaml: "yaml",
	yml: "yaml",
	toml: "toml",
	ini: "ini",
	sql: "sql",
	txt: "plaintext",
	log: "log",
	diff: "diff",
	patch: "diff",
	dockerfile: "dockerfile",
	makefile: "makefile",
	pl: "perl",
	r: "r",
	dart: "dart",
	scala: "scala",
	clj: "clojure",
	ex: "elixir",
	exs: "elixir",
	erl: "erlang",
	hs: "haskell",
	jl: "julia",
	nix: "nix",
	vue: "vue",
	svelte: "svelte",
	tex: "latex",
	graphql: "graphql",
	proto: "proto",
	tf: "terraform",
	csv: "csv",
	tsv: "tsv",
	ipynb: "jupyter",
};

function sections(theme: IconTheme, light: boolean): IconAssociations[] {
	return light && theme.light ? [theme.light, theme] : [theme];
}

function first(
	parts: IconAssociations[],
	pick: (part: IconAssociations) => Record<string, string> | undefined,
	key: string | undefined,
): string | undefined {
	if (key === undefined) return undefined;
	for (const part of parts) {
		const id = pick(part)?.[key];
		if (id) return id;
	}
	return undefined;
}

/**
 * A file's icon, as VS Code finds it: by its whole name, then by its
 * extensions longest first (`test.tsx` before `tsx`), then by its
 * language, else the theme's file icon.
 */
export function fileIconId(theme: IconTheme, path: string, light = false): string | undefined {
	const parts = sections(theme, light);
	const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
	const byName = first(parts, (part) => part.fileNames, name);
	if (byName) return byName;
	const extensions: string[] = [];
	for (let at = name.indexOf("."); at !== -1; at = name.indexOf(".", at + 1))
		if (at + 1 < name.length) extensions.push(name.slice(at + 1));
	for (const extension of extensions) {
		const id = first(parts, (part) => part.fileExtensions, extension);
		if (id) return id;
	}
	const language = LANGUAGES[extensions.at(-1) ?? name] ?? LANGUAGES[name];
	return (
		first(parts, (part) => part.languageIds, language) ?? parts.find((part) => part.file)?.file
	);
}

/** A folder's icon by its name, open or closed; the project's own folder as a root. */
export function folderIconId(
	theme: IconTheme,
	name: string,
	open: boolean,
	{ light = false, root = false }: { light?: boolean; root?: boolean } = {},
): string | undefined {
	const parts = sections(theme, light);
	const key = name.slice(name.lastIndexOf("/") + 1).toLowerCase();
	const named = root
		? first(parts, (part) => (open ? part.rootFolderNamesExpanded : part.rootFolderNames), key)
		: first(parts, (part) => (open ? part.folderNamesExpanded : part.folderNames), key);
	if (named) return named;
	const fallback = root
		? parts.find((part) => (open ? part.rootFolderExpanded : part.rootFolder))
		: undefined;
	if (fallback) return open ? fallback.rootFolderExpanded : fallback.rootFolder;
	return parts.find((part) => (open ? part.folderExpanded : part.folder))?.[
		open ? "folderExpanded" : "folder"
	];
}
