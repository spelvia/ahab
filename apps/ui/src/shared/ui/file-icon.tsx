import { cn } from "cn";
import {
	Braces,
	File,
	FileArchive,
	FileCode,
	FileCog,
	FileImage,
	FileLock,
	FileSpreadsheet,
	FileTerminal,
	FileText,
	FileType,
	Folder,
	FolderOpen,
	type LucideIcon,
} from "lucide-react";
import { useSyncExternalStore } from "react";

import { fileIconId, folderIconId } from "@/shared/file-icons/theme";
import { type FileIcons, fileIcons, rememberFileIcons } from "@/shared/preferences/store";

type Kind = [LucideIcon, string];

const CODE: Kind = [FileCode, "text-sky-500"];
const SCRIPT: Kind = [FileCode, "text-amber-500"];
const RUST: Kind = [FileCode, "text-orange-500"];
const DATA: Kind = [Braces, "text-yellow-500"];
const CONFIG: Kind = [FileCog, "text-muted-foreground"];
const TEXT: Kind = [FileText, "text-muted-foreground"];
const DOC: Kind = [FileText, "text-sky-400"];
const STYLE: Kind = [FileType, "text-fuchsia-500"];
const IMAGE: Kind = [FileImage, "text-emerald-500"];
const SHELL: Kind = [FileTerminal, "text-lime-500"];
const LOCK: Kind = [FileLock, "text-muted-foreground"];
const ARCHIVE: Kind = [FileArchive, "text-muted-foreground"];
const TABLE: Kind = [FileSpreadsheet, "text-emerald-500"];

const BY_EXTENSION: Record<string, Kind> = {
	ts: CODE,
	tsx: CODE,
	mts: CODE,
	cts: CODE,
	js: SCRIPT,
	jsx: SCRIPT,
	mjs: SCRIPT,
	cjs: SCRIPT,
	rs: RUST,
	go: CODE,
	py: CODE,
	rb: CODE,
	java: CODE,
	kt: CODE,
	swift: CODE,
	c: CODE,
	h: CODE,
	cpp: CODE,
	hpp: CODE,
	cs: CODE,
	php: CODE,
	lua: CODE,
	nix: CODE,
	sql: CODE,
	html: CODE,
	vue: CODE,
	svelte: CODE,
	astro: CODE,
	json: DATA,
	jsonc: DATA,
	json5: DATA,
	yaml: CONFIG,
	yml: CONFIG,
	toml: CONFIG,
	ini: CONFIG,
	env: CONFIG,
	conf: CONFIG,
	xml: CONFIG,
	md: DOC,
	mdx: DOC,
	txt: TEXT,
	rst: TEXT,
	css: STYLE,
	scss: STYLE,
	sass: STYLE,
	less: STYLE,
	png: IMAGE,
	jpg: IMAGE,
	jpeg: IMAGE,
	gif: IMAGE,
	svg: IMAGE,
	webp: IMAGE,
	ico: IMAGE,
	avif: IMAGE,
	sh: SHELL,
	bash: SHELL,
	zsh: SHELL,
	fish: SHELL,
	ps1: SHELL,
	lock: LOCK,
	zip: ARCHIVE,
	gz: ARCHIVE,
	tar: ARCHIVE,
	tgz: ARCHIVE,
	xz: ARCHIVE,
	csv: TABLE,
	tsv: TABLE,
};

const BY_NAME: Record<string, Kind> = {
	dockerfile: CONFIG,
	makefile: SHELL,
	justfile: SHELL,
	license: TEXT,
	".gitignore": CONFIG,
	".gitattributes": CONFIG,
	".editorconfig": CONFIG,
	"cargo.lock": LOCK,
	"pnpm-lock.yaml": LOCK,
	"package-lock.json": LOCK,
	"flake.lock": LOCK,
};

function kind(path: string): Kind {
	const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
	const byName = BY_NAME[name];
	if (byName) return byName;
	const dot = name.lastIndexOf(".");
	return (dot > 0 && BY_EXTENSION[name.slice(dot + 1)]) || [File, "text-muted-foreground"];
}

type Material = typeof import("@/shared/file-icons/material");

// The icon theme loads on first use, apart from the app's own code; until
// it has, and when plain icons are chosen, Ahab's own show.
let material: Material | undefined;
let loading: Promise<void> | undefined;
let chosen: FileIcons = fileIcons();
const listeners = new Set<() => void>();
let version = 0;

function changed() {
	version += 1;
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	if (chosen === "material" && !material && !loading)
		loading = import("@/shared/file-icons/material").then((loaded) => {
			material = loaded;
			changed();
		});
	return () => listeners.delete(listener);
}

/** Switches between the bundled icon theme and the plain icons, remembered. */
export function setFileIcons(icons: FileIcons) {
	chosen = icons;
	rememberFileIcons(icons);
	changed();
}

export function currentFileIcons(): FileIcons {
	return chosen;
}

function useTheme(): Material | undefined {
	useSyncExternalStore(
		subscribe,
		() => version,
		() => version,
	);
	return chosen === "material" ? material : undefined;
}

function light(): boolean {
	return typeof document !== "undefined" && !document.documentElement.classList.contains("dark");
}

function Svg({ url, className }: { url: string; className?: string }) {
	return (
		<img
			src={url}
			alt=""
			aria-hidden
			draggable={false}
			className={cn("size-4 shrink-0 select-none", className)}
		/>
	);
}

/** A file's icon, by its name: from the icon theme, else Ahab's own. */
export function FileIcon({ path, className }: { path: string; className?: string }) {
	const theme = useTheme();
	if (theme) {
		const id = fileIconId(theme.theme, path, light());
		const url = id ? theme.iconUrl(id) : undefined;
		if (url) return <Svg url={url} className={className} />;
	}
	const [Icon, tone] = kind(path);
	return <Icon aria-hidden className={cn("size-4 shrink-0", tone, className)} />;
}

/** A folder's icon by its name (`src`, `.github`, `node_modules`…), open or closed. */
export function FolderIcon({
	name,
	open = false,
	className,
}: {
	name: string;
	open?: boolean;
	className?: string;
}) {
	const theme = useTheme();
	if (theme) {
		const id = folderIconId(theme.theme, name, open, { light: light() });
		const url = id ? theme.iconUrl(id) : undefined;
		if (url) return <Svg url={url} className={className} />;
	}
	const Icon = open ? FolderOpen : Folder;
	return <Icon aria-hidden className={cn("text-muted-foreground size-4 shrink-0", className)} />;
}
