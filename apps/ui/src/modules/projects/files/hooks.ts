import { useEffect, useEffectEvent, useSyncExternalStore } from "react";

import { movedPath, type WindowIndentation } from "@/modules/projects/files/model";
import { type FileListener, watchFile } from "@/modules/projects/files/stream";
import type { CursorInfo, LanguageStatus, LineEnding } from "@/shared/editor/model";
import {
	type FileTreeSort,
	fileTreeSort,
	rememberFileTreeSort,
	rememberUnsavedFile,
	type UnsavedFile,
	unsavedFile,
	unsavedFiles,
} from "@/shared/preferences/store";
import type { Language } from "@/shared/syntax/model";

/** Follows a file's version on disk while `enabled`. */
export function useFileWatch(
	projectId: string,
	path: string,
	enabled: boolean,
	onVersion: FileListener,
) {
	const changed = useEffectEvent(onVersion);
	useEffect(() => {
		if (!enabled) return;
		return watchFile(projectId, path, (version) => changed(version));
	}, [projectId, path, enabled]);
}

// Files with unsaved changes, kept in this browser, so a closed
// window loses nothing and tabs can mark them.

const listeners = new Set<() => void>();
let marked: Set<string> | undefined;

function current(): Set<string> {
	marked ??= new Set(unsavedFiles());
	return marked;
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** Keeping and forgetting files' unsaved changes. */
export const unsavedStore = {
	get: (projectId: string, path: string) => unsavedFile(projectId, path),
	keep(projectId: string, path: string, file: UnsavedFile) {
		rememberUnsavedFile(projectId, path, file);
		const key = `${projectId}:${path}`;
		if (current().has(key)) return;
		marked = new Set([...current(), key]);
		for (const listener of listeners) listener();
	},
	forget(projectId: string, path: string) {
		rememberUnsavedFile(projectId, path, undefined);
		const key = `${projectId}:${path}`;
		if (!current().has(key)) return;
		const next = new Set(current());
		next.delete(key);
		marked = next;
		for (const listener of listeners) listener();
	},
};

// Files moved in the tree (the rename, or dragging): open windows
// keep their unsaved changes under the new path first.

const moveListeners = new Set<(projectId: string, from: string, to: string) => void>();

/**
 * Tells open windows that `from` (a file, or a folder and everything in it)
 * moved to `to`, then moves the unsaved changes kept under it.
 */
export function announceMove(projectId: string, from: string, to: string) {
	for (const listener of moveListeners) listener(projectId, from, to);
	for (const key of current()) {
		const prefix = `${projectId}:`;
		if (!key.startsWith(prefix)) continue;
		const path = key.slice(prefix.length);
		const moved = movedPath(path, from, to);
		if (moved === path) continue;
		const kept = unsavedStore.get(projectId, path);
		if (kept) unsavedStore.keep(projectId, moved, kept);
		unsavedStore.forget(projectId, path);
	}
}

/** Calls `onMoved` before an announced move takes this file away. */
export function useMovedAway(projectId: string, path: string, onMoved: () => void) {
	const moved = useEffectEvent(onMoved);
	useEffect(() => {
		const listener = (project: string, from: string, to: string) => {
			if (project === projectId && movedPath(path, from, to) !== path) moved();
		};
		moveListeners.add(listener);
		return () => {
			moveListeners.delete(listener);
		};
	}, [projectId, path]);
}

/** Whether a file has unsaved changes, kept up to date. */
export function useUnsaved(projectId: string, path: string): boolean {
	return useSyncExternalStore(
		subscribe,
		() => current().has(`${projectId}:${path}`),
		() => false,
	);
}

// What the focused file window tells the status bar: where the cursor is,
// how the text indents, ends its lines and is encoded, its language and
// tooling, and whether it is being edited; with what each item does when
// clicked. Kept per window (by `fileStatusKey`); the workbench shows the
// focused window's.

/** A window's language, and the menu to choose another. */
interface FileLanguage {
	languages: Language[];
	detected: Language | undefined;
	chosen: string | undefined;
	pattern: string;
	patternChosen: boolean;
	note?: string;
	choose: (language: string | undefined) => void;
	choosePattern: (language: string | undefined) => void;
}

/** Whether a window edits its file, and why it cannot. */
export type FileEditState =
	| "editing"
	| "readOnly"
	/** It could be edited: the Edit switch is off. */
	| "viewing"
	| "readOnlyProject"
	| "truncated"
	| "external";

export interface FileStatus {
	indentation: WindowIndentation;
	/** `UTF-8` or `UTF-8 with BOM`: what the text was read as. */
	encoding: string;
	lineEnding: { ending: LineEnding; mixed: boolean };
	/** The language's name, when there is no menu to choose another. */
	languageName?: string;
	language?: FileLanguage;
	languageStatus: LanguageStatus | null;
	edit: FileEditState;
	toggleEdit?: () => void;
	goToLine?: () => void;
	/** F8, ⇧F8: the next or previous problem in the file. */
	goToProblem?: (direction: 1 | -1) => void;
	/** Indents this window's text so; null goes back to finding it. */
	setIndentation?: (choice: { useTabs: boolean; tabSize: number } | null) => void;
	/** Converts the text's line endings (Edit mode only). */
	setLineEnding?: (ending: LineEnding) => void;
}

interface StatusEntry {
	status?: FileStatus;
	cursor?: CursorInfo;
}

const statuses = new Map<string, StatusEntry>();
const statusListeners = new Set<() => void>();

function changeStatus(key: string, change: (entry: StatusEntry) => StatusEntry | undefined) {
	const next = change(statuses.get(key) ?? {});
	if (next) statuses.set(key, next);
	else statuses.delete(key);
	for (const listener of statusListeners) listener();
}

/** The key a file window's status is kept under. */
export function fileStatusKey(projectId: string, path: string): string {
	return `${projectId}:${path}`;
}

/** Publishing a window's status, and taking it back. */
export const fileStatusStore = {
	set: (key: string, status: FileStatus) => changeStatus(key, (entry) => ({ ...entry, status })),
	cursor: (key: string, cursor: CursorInfo) => changeStatus(key, (entry) => ({ ...entry, cursor })),
	remove: (key: string) => changeStatus(key, () => undefined),
};

const NO_STATUS: StatusEntry = {};

/** A file window's status now, outside React (the palette's commands). */
export function fileStatusOf(key: string | undefined): FileStatus | undefined {
	return key ? statuses.get(key)?.status : undefined;
}

/** A file window's status, kept up to date; empty while there is none. */
export function useFileStatus(key: string | undefined): StatusEntry {
	return useSyncExternalStore(
		(listener) => {
			statusListeners.add(listener);
			return () => statusListeners.delete(listener);
		},
		() => (key ? (statuses.get(key) ?? NO_STATUS) : NO_STATUS),
		() => NO_STATUS,
	);
}

// --- How the file tree orders folders, on this device ------------------------

let sortNow = fileTreeSort();
const sortListeners = new Set<() => void>();

/** The file tree's order, re-rendering every folder when it changes. */
export function useFileSort(): FileTreeSort {
	return useSyncExternalStore(
		(listener) => {
			sortListeners.add(listener);
			return () => sortListeners.delete(listener);
		},
		() => sortNow,
	);
}

export function changeFileSort(sort: FileTreeSort) {
	sortNow = sort;
	rememberFileTreeSort(sort);
	for (const listener of sortListeners) listener();
}
