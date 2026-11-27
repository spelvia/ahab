import { t } from "@/shared/i18n";

/** What a window does when its file's version on disk changes. */
export type DiskChange =
	/** The version it shows (or just saved). */
	| "same"
	/** No unsaved changes: show the new content. */
	| "reload"
	/** Unsaved changes: say so, and offer Compare, Reload, Keep mine. */
	| "conflict"
	/** The file is gone. */
	| "deleted";

/**
 * How a window answers its file's new version on disk, given the version
 * its text started from and whether it has unsaved changes.
 */
export function diskChange(
	base: string | null | undefined,
	dirty: boolean,
	disk: string | null,
): DiskChange {
	if (disk === null) return base === null ? "same" : "deleted";
	if (disk === base) return "same";
	return dirty ? "conflict" : "reload";
}

/** Whether a file can be edited: text, whole, with a version to save over, in a writable project. */
export function editable(
	file: { binary: boolean; truncated?: boolean; version?: string | null },
	readOnlyProject: boolean,
): boolean {
	return !readOnlyProject && !file.binary && !file.truncated && Boolean(file.version);
}

/** Why a name for a new or renamed file is refused; nothing when it is fine. */
export function badName(name: string): string | undefined {
	const trimmed = name.trim();
	if (!trimmed) return t("filesModel.badName.empty", { ns: "projects" });
	if (trimmed.startsWith("/")) return t("filesModel.badName.absolute", { ns: "projects" });
	if (trimmed.split("/").some((part) => part === ".." || part === "."))
		return t("filesModel.badName.dots", { ns: "projects" });
	if (/[\\\0]/.test(trimmed)) return t("filesModel.badName.backslash", { ns: "projects" });
	return undefined;
}

/** The folder part of a path, "" for the root. */
export function folderOf(path: string): string {
	const at = path.lastIndexOf("/");
	return at === -1 ? "" : path.slice(0, at);
}

/** `folder/name`, or `name` in the root. */
export function joinPath(folder: string, name: string): string {
	const clean = name.trim().replace(/^\/+/, "");
	return folder ? `${folder}/${clean}` : clean;
}

/** A path's last part. */
export function nameOf(path: string): string {
	return path.slice(path.lastIndexOf("/") + 1);
}

/** One file or folder to move (or copy) by dragging. */
export interface Move {
	from: string;
	to: string;
}

/**
 * The paths not inside another of them, once each and in their order:
 * what acting on several chosen files and folders touches.
 */
export function topLevelPaths(paths: readonly string[]): string[] {
	return paths.filter(
		(path, index) =>
			path &&
			paths.indexOf(path) === index &&
			!paths.some((other) => other !== path && path.startsWith(`${other}/`)),
	);
}

/** The items of `order` from `from` to `to`, both included, either way round. */
export function rangeOf<T>(order: readonly T[], from: T, to: T): T[] {
	const start = order.indexOf(from);
	const end = order.indexOf(to);
	if (end === -1) return [];
	if (start === -1) return [to];
	return order.slice(Math.min(start, end), Math.max(start, end) + 1);
}

/**
 * Where typing `typed` in a list of `names` goes from `current`: the next
 * name that starts with it, case aside, wrapping round; one letter typed
 * again moves on to the next such name, a longer prefix stays while it
 * still matches; the same letter typed over and over moves on each time.
 * -1 when none matches.
 */
export function typeAhead(names: readonly string[], typed: string, current: number): number {
	const prefix = typed.toLowerCase();
	if (!prefix) return -1;
	const from = prefix.length === 1 ? current + 1 : Math.max(current, 0);
	for (let step = 0; step < names.length; step++) {
		const index = (from + step) % names.length;
		if (names[index]?.toLowerCase().startsWith(prefix)) return index;
	}
	const letter = prefix[0] ?? "";
	return prefix.length > 1 && [...prefix].every((each) => each === letter)
		? typeAhead(names, letter, current)
		: -1;
}

/**
 * What dropping `sources` onto `folder` ("" for the project's own) moves:
 * each into it under its name. What is inside another source goes with it,
 * what is in `folder` already stays; `refused` when a folder would go into
 * itself.
 */
export function planMoves(
	sources: readonly string[],
	folder: string,
): { moves: Move[]; refused?: string } {
	const top = topLevelPaths(sources);
	const into = top.find((path) => folder === path || folder.startsWith(`${path}/`));
	if (into)
		return {
			moves: [],
			refused: t("filesModel.intoItself", { ns: "projects", name: nameOf(into) }),
		};
	return {
		moves: top
			.filter((path) => folderOf(path) !== folder)
			.map((path) => ({ from: path, to: joinPath(folder, nameOf(path)) })),
	};
}

/**
 * A name beside `name` that is free, as Keep both chooses it: `a copy.ts`,
 * then `a copy 2.ts`; folders and dotfiles take it at the end.
 */
export function keepBothName(name: string, taken: (name: string) => boolean): string {
	const dot = name.lastIndexOf(".");
	const [stem, extension] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
	for (let n = 1; ; n += 1) {
		const candidate = `${stem} copy${n === 1 ? "" : ` ${n}`}${extension}`;
		if (!taken(candidate)) return candidate;
	}
}

/** Where `path` is after `from` moved to `to`; itself when not under it. */
export function movedPath(path: string, from: string, to: string): string {
	if (path === from) return to;
	if (path.startsWith(`${from}/`)) return to + path.slice(from.length);
	return path;
}

/** What EditorConfig says of a file's indentation. */
export interface EditorConfig {
	tabSize?: number;
	useTabs?: boolean;
}

/** Whether an EditorConfig section's glob matches `path` (relative to the file's folder). */
export function editorConfigMatches(glob: string, path: string): boolean {
	const pattern = glob.includes("/") ? glob.replace(/^\//, "") : `**/${glob}`;
	let regex = "";
	for (let index = 0; index < pattern.length; index++) {
		const char = pattern[index] ?? "";
		if (char === "*" && pattern[index + 1] === "*") {
			regex += ".*";
			index++;
			if (pattern[index + 1] === "/") {
				regex = regex.slice(0, -2) + "(?:.*/)?";
				index++;
			}
		} else if (char === "*") regex += "[^/]*";
		else if (char === "?") regex += "[^/]";
		else if (char === "{") {
			const close = pattern.indexOf("}", index);
			const choices = pattern.slice(index + 1, close).split(",");
			regex += `(?:${choices.map((choice) => choice.replace(/[.+^$()|[\]\\]/g, "\\$&")).join("|")})`;
			index = close;
		} else regex += char.replace(/[.+^$()|[\]\\]/g, "\\$&");
	}
	return new RegExp(`^${regex}$`).test(path);
}

/** A width from 1 to 16, else none. */
function width(value?: string): number | undefined {
	const parsed = Number(value);
	return Number.isInteger(parsed) && parsed > 0 && parsed <= 16 ? parsed : undefined;
}

/**
 * EditorConfig for a file, from `.editorconfig` texts nearest first (each
 * with the file's path relative to its folder): nearer files win, a
 * `root = true` stops the search.
 */
export function editorConfig(files: { text: string; path: string }[]): EditorConfig {
	const found: EditorConfig = {};
	let size: string | undefined;
	let tabWidth: string | undefined;
	for (const file of files) {
		let applies = false;
		let root = false;
		const values: Record<string, string> = {};
		for (const raw of file.text.split("\n")) {
			const line = raw.trim();
			if (!line || line.startsWith("#") || line.startsWith(";")) continue;
			const section = /^\[(.*)\]$/.exec(line);
			if (section) {
				applies = editorConfigMatches(section[1] ?? "", file.path);
				continue;
			}
			const [key, ...rest] = line.split("=");
			const name = key?.trim().toLowerCase() ?? "";
			const value = rest.join("=").trim().toLowerCase();
			if (name === "root" && value === "true" && !applies) root = true;
			else if (applies) values[name] = value;
		}
		// Nearer files were read first: they keep what they set.
		if (found.useTabs === undefined && values.indent_style)
			found.useTabs = values.indent_style === "tab";
		size ??= values.indent_size;
		tabWidth ??= values.tab_width;
		if (root) break;
	}
	// Tabs show as wide as `tab_width`; spaces indent by `indent_size`.
	found.tabSize = found.useTabs
		? (width(tabWidth) ?? width(size))
		: (width(size === "tab" ? tabWidth : size) ?? width(tabWidth));
	if (found.tabSize === undefined) delete found.tabSize;
	if (found.useTabs === undefined) delete found.useTabs;
	return found;
}

/** Where a window's indentation comes from, for the status bar. */
type IndentationSource = "window" | "editorconfig" | "detected" | "settings";

/** A window's indentation, and what decided it. */
export interface WindowIndentation {
	useTabs: boolean;
	tabSize: number;
	source: IndentationSource;
}

/**
 * A window's indentation, as VS Code decides it: chosen for the window
 * (the status bar) over EditorConfig, over what the text itself uses, over
 * the editor settings. A text indented with tabs keeps the settings' tab
 * size.
 */
export function resolveIndentation({
	settings,
	editorconfig,
	detected,
	chosen,
}: {
	settings: { tabSize: number; useTabs?: boolean };
	editorconfig?: EditorConfig;
	detected?: { useTabs: boolean; size: number } | null;
	chosen?: { useTabs: boolean; tabSize: number } | null;
}): WindowIndentation {
	if (chosen) return { ...chosen, source: "window" };
	let found: WindowIndentation = {
		useTabs: Boolean(settings.useTabs),
		tabSize: settings.tabSize,
		source: "settings",
	};
	if (detected)
		found = detected.useTabs
			? { ...found, useTabs: true, source: "detected" }
			: { useTabs: false, tabSize: detected.size, source: "detected" };
	if (editorconfig?.useTabs !== undefined || editorconfig?.tabSize !== undefined)
		found = {
			useTabs: editorconfig.useTabs ?? found.useTabs,
			tabSize: editorconfig.tabSize ?? found.tabSize,
			source: "editorconfig",
		};
	return found;
}

/** How a folder's entries are ordered (Settings kept on this device). */
export interface EntrySort {
	by: "name" | "type" | "modified" | "size";
	foldersFirst: boolean;
}

const byName = (a: { name: string }, b: { name: string }) =>
	a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true });

/** The extension a name ends with, lower-case; none for dotfiles. */
function extensionOf(name: string): string {
	const dot = name.lastIndexOf(".");
	return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * A folder's entries in `sort`'s order: by name, by type (extension), newest
 * first, or largest first; ties by name. Folders first when asked.
 */
export function sortEntries<
	Entry extends { name: string; kind: string; modifiedAt?: string | null; size?: number | null },
>(entries: readonly Entry[], sort: EntrySort): Entry[] {
	const order = (a: Entry, b: Entry): number => {
		switch (sort.by) {
			case "type":
				return extensionOf(a.name).localeCompare(extensionOf(b.name));
			case "modified":
				return (b.modifiedAt ?? "").localeCompare(a.modifiedAt ?? "");
			case "size":
				return (b.size ?? 0) - (a.size ?? 0);
			default:
				return 0;
		}
	};
	return entries.toSorted((a, b) => {
		if (sort.foldersFirst && (a.kind === "dir") !== (b.kind === "dir"))
			return a.kind === "dir" ? -1 : 1;
		return order(a, b) || byName(a, b);
	});
}
