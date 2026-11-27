/**
 * What this browser remembers between visits: the last project, and each
 * project's sidebar view. The only place that touches localStorage.
 */

import type { SendMessage } from "@/shared/api/generated/ahab.schemas";

const KEY = "ahab.preferences.v1";

interface Preferences {
	/** The last project's ID. */
	lastProject?: string;
	lastAgent?: string;
	/** Session folders shown closed, by ID. */
	closedFolders?: string[];
	/** How the Sessions view orders and groups sessions. */
	sessionListView?: SessionListView;
	/** How the file tree orders each folder's entries. */
	fileTreeSort?: FileTreeSort;
	/** A sound as notifications arrive, on this device. */
	notificationSound?: NotificationSound;
	/** Which views each sidebar holds, which shows, whether it is open, its width. */
	sidebars?: Sidebars;
	/** How markdown files show: their source, rendered, or both. */
	markdownView?: MarkdownView;
	/** Files opened lately, per project, newest first (quick open). */
	recentFiles?: Record<string, string[]>;
	/** The Git repository chosen inside each project's folder. */
	gitRepositories?: Record<string, string>;
	/** Whether the Git view shows changes as a tree of folders, not a list. */
	gitTree?: boolean;
	/** Whether the Git view's graph shows every branch, not only the current one. */
	gitGraphAll?: boolean;
	/** The Git view's graph pane's height, in pixels. */
	gitGraphHeight?: number;
	/** File icons: the bundled icon theme, or Ahab's plain ones. */
	fileIcons?: FileIcons;
	/** The Search view's last query and options, per project. */
	searches?: Record<string, SavedSearch>;
	/** Shortcuts changed from Ahab's: each action's keys, in place of its own. */
	shortcuts?: Record<string, string[]>;
	/**
	 * Clients whose "not used for 3 months" notice was dismissed, by ID. Kept
	 * under their former name, devices, so what was stored still reads.
	 */
	dismissedUnusedDevices?: string[];
	/** A file window's language chosen by hand, by `project:path`. */
	fileLanguages?: Record<string, string>;
	/** A language chosen for every file of a pattern (`*.h`), over the one found. */
	patternLanguages?: Record<string, string>;
	/** Whether file windows show their outline. */
	outline?: boolean;
	/** The bottom panel (Problems, Output): open per project, its height, tab and choices. */
	bottomPanel?: BottomPanel;
	/** This browser's Web Push endpoint, while it gets pushes. */
	pushEndpoint?: string;
	/** Whether the phone app asked once to tell about agents. */
	askedPhoneNotifications?: boolean;
	/** Each session's window: the tab in front, the one beside it, tabs closed. */
	sessionViews?: Record<string, SessionView>;
	/** Sessions opened in the Sessions view's tree, by ID. */
	openSessions?: string[];
	/** Settings → Appearance: theme, text size, motion and language, on this device. */
	appearance?: StoredAppearance;
	/** Vim key mappings of this device (Settings → Keyboard), in order. */
	vimMappings?: VimMapping[];
	/** Folders open in each project's file tree, by project ID. */
	openFileFolders?: Record<string, string[]>;
}

/** A Vim key mapping, as `:map` makes one: `jk` → `<Esc>` in insert mode. */
export interface VimMapping {
	/** `all` is `:map` (normal, visual and operator-pending). */
	mode: "normal" | "visual" | "insert" | "all";
	/** The keys typed, in Vim's notation: `jk`, `<C-s>`. */
	lhs: string;
	/** The keys they stand for: `<Esc>`, `:w<CR>`. */
	rhs: string;
	/** `:map` rather than `:noremap`: the keys stood for may be mappings themselves. */
	recursive?: boolean;
}

/** How a session's window shows its chats and terminals, on this device. */
export interface SessionView {
	/** The tab in front (`chat:<id>`, `terminal:<id>`, or `new` for a New chat form). */
	active?: string;
	/** The tab shown beside it, when split. */
	beside?: string;
	/** Tabs closed: still in the session, shown again when opened. */
	hidden?: string[];
}

/** What Settings → Appearance keeps; `index.html` reads `theme` before the first paint. */
export interface StoredAppearance {
	theme?: "system" | "light" | "dark";
	textPercent?: number;
	/** Earlier versions stored one of four named sizes. */
	textSize?: "small" | "default" | "large" | "larger";
	motion?: "system" | "reduce";
	language?: string;
	/** Agents' thinking shown open in conversations. */
	expandThinking?: boolean;
	/** Commands and other tool calls shown open in conversations. */
	expandTools?: boolean;
	/** A chat's session cost shown in its Session usage. */
	showCost?: boolean;
	/** The tokens a chat's session spent shown in its Session usage. */
	showTokens?: boolean;
	/** A built-in colour theme's ID; none for Ahab's own. */
	colorTheme?: string;
	/** The interface's and code's fonts: a choice's ID or a font's name; empty for Ahab's. */
	uiFont?: string;
	codeFont?: string;
}

/** What the bottom panel keeps between visits. */
export interface BottomPanel {
	/** Open or closed, by project ID. */
	open?: Record<string, boolean>;
	/** The view as last arranged in each project. */
	views?: Record<string, { height: number; maximised: boolean; tab: string }>;
	/** Defaults from versions that stored the view across projects. */
	height?: number;
	maximised?: boolean;
	tab?: string;
	/** The Output tab's source, by project ID. */
	output?: Record<string, string>;
}

export interface SavedSearch {
	query: string;
	matchCase: boolean;
	wholeWord: boolean;
	regex: boolean;
	include: string;
	exclude: string;
	includeIgnored: boolean;
}

export type FileIcons = "material" | "minimal";

export type MarkdownView = "source" | "preview" | "split";

/** One sidebar: its views in order, the one shown, open or not, its width. */
export interface Sidebar {
	views: string[];
	active?: string;
	open: boolean;
	width: number;
}

export interface Sidebars {
	left: Sidebar;
	right: Sidebar;
}

function read(): Preferences {
	try {
		const raw = window.localStorage.getItem(KEY);
		return raw ? (JSON.parse(raw) as Preferences) : {};
	} catch {
		return {};
	}
}

function write(change: (current: Preferences) => Preferences) {
	try {
		window.localStorage.setItem(KEY, JSON.stringify(change(read())));
	} catch {
		// Private windows may refuse storage; remembering is a convenience.
	}
}

export function lastProject(): string | undefined {
	return read().lastProject;
}

export function rememberProject(project: string) {
	write((current) => ({ ...current, lastProject: project }));
}

export function lastAgent(): string | undefined {
	return read().lastAgent;
}

export function rememberAgent(agent: string) {
	write((current) => ({ ...current, lastAgent: agent }));
}

/** Whether this device plays a sound as a notification arrives, and which. */
export interface NotificationSound {
	on: boolean;
	tone: "chime" | "ping" | "soft";
	/** 0 to 1. */
	volume: number;
}

export function notificationSound(): NotificationSound {
	return { on: true, tone: "chime", volume: 0.6, ...read().notificationSound };
}

export function rememberNotificationSound(sound: NotificationSound) {
	write((current) => ({ ...current, notificationSound: sound }));
}

/** How the file tree orders a folder, on this device. */
export interface FileTreeSort {
	by: "name" | "type" | "modified" | "size";
	/** Folders before files, whatever the order. */
	foldersFirst: boolean;
}

export function fileTreeSort(): FileTreeSort {
	return { by: "name", foldersFirst: true, ...read().fileTreeSort };
}

export function rememberFileTreeSort(sort: FileTreeSort) {
	write((current) => ({ ...current, fileTreeSort: sort }));
}

/** How the Sessions view lists sessions, on this device. */
export interface SessionListView {
	sort: "activity" | "created" | "name";
	group: "folders" | "none" | "status" | "agent" | "day";
}

export function sessionListView(): SessionListView {
	return { sort: "activity", group: "folders", ...read().sessionListView };
}

export function rememberSessionListView(view: SessionListView) {
	write((current) => ({ ...current, sessionListView: view }));
}

export function closedFolders(): string[] {
	return read().closedFolders ?? [];
}

export function rememberFolderClosed(folder: string, closed: boolean) {
	write((current) => {
		const rest = (current.closedFolders ?? []).filter((id) => id !== folder);
		return { ...current, closedFolders: closed ? [...rest, folder] : rest };
	});
}

export function sidebars(): Sidebars | undefined {
	return read().sidebars;
}

export function rememberSidebars(layout: Sidebars) {
	write((current) => ({ ...current, sidebars: layout }));
}

export function bottomPanel(): BottomPanel {
	return read().bottomPanel ?? {};
}

export function rememberBottomPanel(change: (current: BottomPanel) => BottomPanel) {
	write((current) => ({ ...current, bottomPanel: change(current.bottomPanel ?? {}) }));
}

/** An arrangement that has yet to reach the server. */
export type PendingLayout = { layout: Record<string, unknown>; revision: number };

export function pendingProjectLayout(key: string): PendingLayout | undefined {
	try {
		const name = `ahab.layout.pending:${key}`;
		const raw = sessionStorage.getItem(name) ?? localStorage.getItem(name);
		if (!raw) return undefined;
		const value = JSON.parse(raw) as PendingLayout;
		if (Number.isSafeInteger(value.revision) && value.layout && typeof value.layout === "object")
			return value;
	} catch {
		// Storage can be unavailable or contain an unreadable snapshot.
	}
	return undefined;
}

export function rememberPendingLayout(key: string, snapshot: PendingLayout | undefined) {
	try {
		const name = `ahab.layout.pending:${key}`;
		if (snapshot) {
			const text = JSON.stringify(snapshot);
			sessionStorage.setItem(name, text);
			localStorage.setItem(name, text);
		} else {
			const staged = sessionStorage.getItem(name);
			if (!staged || localStorage.getItem(name) === staged) localStorage.removeItem(name);
			sessionStorage.removeItem(name);
		}
	} catch {
		// The layout store keeps the snapshot in memory for this visit.
	}
}

export type PendingMessage = SendMessage & { submissionId: string };

export function pendingChatMessage(chat: string): PendingMessage | undefined {
	try {
		const name = `ahab.chat.submission:${chat}`;
		const raw = sessionStorage.getItem(name) ?? localStorage.getItem(name);
		if (raw) {
			const value = JSON.parse(raw) as PendingMessage;
			if (typeof value?.submissionId === "string") return value;
		}
	} catch {
		// Storage may be unavailable. The composer still holds the active draft.
	}
	return undefined;
}

export function rememberPendingChatMessage(chat: string, message: PendingMessage | undefined) {
	try {
		const key = `ahab.chat.submission:${chat}`;
		if (message) {
			const text = JSON.stringify(message);
			sessionStorage.setItem(key, text);
			localStorage.setItem(key, text);
		} else {
			const staged = sessionStorage.getItem(key);
			if (!staged || localStorage.getItem(key) === staged) localStorage.removeItem(key);
			sessionStorage.removeItem(key);
		}
	} catch {
		// The chat retains its submission for the active visit.
	}
}

export function markdownView(): MarkdownView {
	return read().markdownView ?? "preview";
}

export function rememberMarkdownView(view: MarkdownView) {
	write((current) => ({ ...current, markdownView: view }));
}

export function recentFiles(project: string): string[] {
	return read().recentFiles?.[project] ?? [];
}

export function rememberFileOpened(project: string, path: string) {
	write((current) => {
		const list = [path, ...(current.recentFiles?.[project] ?? []).filter((p) => p !== path)];
		return { ...current, recentFiles: { ...current.recentFiles, [project]: list.slice(0, 30) } };
	});
}

export function gitRepository(project: string): string | undefined {
	return read().gitRepositories?.[project];
}

export function rememberGitRepository(project: string, path: string) {
	write((current) => ({
		...current,
		gitRepositories: { ...current.gitRepositories, [project]: path },
	}));
}

export function gitTree(): boolean {
	return read().gitTree ?? false;
}

export function rememberGitTree(on: boolean) {
	write((current) => ({ ...current, gitTree: on }));
}

export function gitGraphAll(): boolean {
	return read().gitGraphAll ?? true;
}

export function rememberGitGraphAll(on: boolean) {
	write((current) => ({ ...current, gitGraphAll: on }));
}

export function gitGraphHeight(): number {
	return read().gitGraphHeight ?? 280;
}

export function rememberGitGraphHeight(height: number) {
	write((current) => ({ ...current, gitGraphHeight: height }));
}

export function fileIcons(): FileIcons {
	return read().fileIcons ?? "material";
}

export function rememberFileIcons(icons: FileIcons) {
	write((current) => ({ ...current, fileIcons: icons }));
}

export function savedSearch(project: string): SavedSearch | undefined {
	return read().searches?.[project];
}

export function rememberSearch(project: string, search: SavedSearch) {
	write((current) => ({ ...current, searches: { ...current.searches, [project]: search } }));
}

export function shortcuts(): Record<string, string[]> {
	return read().shortcuts ?? {};
}

export function rememberShortcuts(changed: Record<string, string[]>) {
	write((current) => ({ ...current, shortcuts: changed }));
}

export function vimMappings(): VimMapping[] {
	const stored = read().vimMappings;
	return Array.isArray(stored) ? stored : [];
}

export function rememberVimMappings(mappings: VimMapping[]) {
	write((current) => ({ ...current, vimMappings: mappings }));
}

export function dismissedUnusedClients(): string[] {
	return read().dismissedUnusedDevices ?? [];
}

export function rememberUnusedClientsDismissed(clients: string[]) {
	write((current) => ({ ...current, dismissedUnusedDevices: clients }));
}

/** The language chosen for a file's window, if one was. */
export function fileLanguage(project: string, path: string): string | undefined {
	return read().fileLanguages?.[`${project}:${path}`];
}

/** Remembers a file window's language, or forgets it (`undefined`: found by itself). */
export function rememberFileLanguage(project: string, path: string, language: string | undefined) {
	write((current) => {
		const languages = { ...current.fileLanguages };
		if (language) languages[`${project}:${path}`] = language;
		else delete languages[`${project}:${path}`];
		return { ...current, fileLanguages: languages };
	});
}

/** Languages chosen for patterns of file names (`*.h` → `cpp`). */
export function patternLanguages(): Record<string, string> {
	return read().patternLanguages ?? {};
}

export function rememberPatternLanguage(pattern: string, language: string | undefined) {
	write((current) => {
		const languages = { ...current.patternLanguages };
		if (language) languages[pattern] = language;
		else delete languages[pattern];
		return { ...current, patternLanguages: languages };
	});
}

export function sessionView(session: string): SessionView {
	return read().sessionViews?.[session] ?? {};
}

export function rememberSessionView(session: string, view: SessionView) {
	write((current) => ({ ...current, sessionViews: { ...current.sessionViews, [session]: view } }));
}

export function openSessions(): string[] {
	return read().openSessions ?? [];
}

export function rememberSessionOpen(session: string, open: boolean) {
	write((current) => {
		const others = (current.openSessions ?? []).filter((id) => id !== session);
		return { ...current, openSessions: open ? [...others, session] : others };
	});
}

export function storedAppearance(): StoredAppearance {
	return read().appearance ?? {};
}

export function rememberAppearance(appearance: StoredAppearance) {
	write((current) => ({ ...current, appearance }));
}

export function outlineShown(): boolean {
	return read().outline ?? false;
}

export function rememberOutlineShown(shown: boolean) {
	write((current) => ({ ...current, outline: shown }));
}

/** The folders open in a project's file tree. */
export function openFileFolders(project: string): string[] {
	return read().openFileFolders?.[project] ?? [];
}

export function rememberOpenFileFolders(project: string, folders: string[]) {
	write((current) => ({
		...current,
		openFileFolders: { ...current.openFileFolders, [project]: folders },
	}));
}

// ---------------------------------------------------------------------------
// The editor: each project's chat target, composers' context
// drafts, and unsaved changes. Kept apart from the preferences above, as
// they can be large.

const TARGETS = "ahab.chatTargets.v1";
const DRAFTS = "ahab.chatDrafts.v1";
const UNSAVED = "ahab.unsaved.v1:";

function readJson<T>(key: string): T | undefined {
	try {
		const raw = window.localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : undefined;
	} catch {
		return undefined;
	}
}

function writeJson(key: string, value: unknown) {
	try {
		if (value === undefined) window.localStorage.removeItem(key);
		else window.localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Full or refused: keeping it is a convenience.
	}
}

/** The session context goes to in a project: the one used last. */
export function chatTarget(project: string): string | undefined {
	return readJson<Record<string, string>>(TARGETS)?.[project];
}

export function rememberChatTarget(project: string, session: string) {
	writeJson(TARGETS, { ...readJson<Record<string, string>>(TARGETS), [project]: session });
}

/** Each composer's context not sent yet, by session (or New session window). */
export function chatDrafts<T>(): Record<string, T[]> {
	return readJson<Record<string, T[]>>(DRAFTS) ?? {};
}

export function rememberChatDrafts<T>(drafts: Record<string, T[]>) {
	const kept = Object.fromEntries(Object.entries(drafts).filter(([, list]) => list.length > 0));
	writeJson(DRAFTS, Object.keys(kept).length ? kept : undefined);
}

const COMPOSER = "ahab.composer.v1:";

export function composerDraft<T>(
	key: string,
	part: "text" | "attachments" | "command" | "arguments",
): T | undefined {
	return readJson<T>(`${COMPOSER}${part}:${key}`);
}

export function rememberComposerDraft(
	key: string,
	part: "text" | "attachments" | "command" | "arguments",
	value: unknown,
) {
	writeJson(`${COMPOSER}${part}:${key}`, value);
}

/** A file's changes not saved yet, and the version they started from. */
export interface UnsavedFile {
	text: string;
	version: string | null;
	/** When they were kept, in milliseconds. */
	at: number;
}

function unsavedKey(project: string, path: string): string {
	return `${UNSAVED}${project}:${path}`;
}

export function unsavedFile(project: string, path: string): UnsavedFile | undefined {
	return readJson<UnsavedFile>(unsavedKey(project, path));
}

/** Keeps a file's unsaved changes, or forgets them (`undefined`). */
export function rememberUnsavedFile(project: string, path: string, file: UnsavedFile | undefined) {
	writeJson(unsavedKey(project, path), file);
}

/** The files with unsaved changes, as `project:path`. */
export function unsavedFiles(): string[] {
	try {
		const found: string[] = [];
		for (let index = 0; index < window.localStorage.length; index++) {
			const key = window.localStorage.key(index);
			if (key?.startsWith(UNSAVED)) found.push(key.slice(UNSAVED.length));
		}
		return found;
	} catch {
		return [];
	}
}

/** This browser's Web Push endpoint, while it gets pushes. */
export function pushEndpoint(): string | undefined {
	return read().pushEndpoint;
}

export function rememberPushEndpoint(endpoint: string | undefined) {
	write((current) => ({ ...current, pushEndpoint: endpoint }));
}

/** Whether the phone app already asked to show notifications. */
export function askedPhoneNotifications(): boolean {
	return read().askedPhoneNotifications ?? false;
}

export function rememberAskedPhoneNotifications() {
	write((current) => ({ ...current, askedPhoneNotifications: true }));
}
