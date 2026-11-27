/**
 * Context for an agent's chat: lines of a file, a whole file, a
 * diff hunk, terminal output or search results, added from wherever they
 * are shown to a session's composer, and sent with the next message. The
 * places that offer context know only this; the sessions module keeps the
 * drafts and sends them.
 */

import type { ContextReference as WireReference } from "@/shared/api/generated/ahab.schemas";
import { type LineSelection, linesLabel } from "@/shared/editor/model";
import { t } from "@/shared/i18n";

export type ReferenceKind =
	| "lines"
	| "file"
	| "diffHunk"
	| "terminalOutput"
	| "searchResults"
	| "chatHistory";

/** One piece of context, as a composer holds it. */
export interface ContextReference {
	/** This browser's own ID for it, to remove or undo it. */
	id: string;
	kind: ReferenceKind;
	/** The project a file is in. */
	projectId?: string;
	/** Relative to the project. */
	path?: string;
	startLine?: number;
	endLine?: number;
	/** The text as it was seen; empty for a whole file. */
	text: string;
	/** The file's version when it was selected. */
	version?: string | null;
	/** A terminal's title, a search's query, a chat's title. */
	label?: string;
}

let counter = 0;
/** A new reference's ID. */
export function referenceId(): string {
	counter += 1;
	return `${Date.now().toString(36)}-${counter.toString(36)}`;
}

/** Each selected run of lines, as a reference. */
export function linesReferences(
	projectId: string,
	path: string,
	version: string | null | undefined,
	lines: readonly LineSelection[],
): ContextReference[] {
	return lines.map((selection) => ({
		id: referenceId(),
		kind: "lines",
		projectId,
		path,
		startLine: selection.startLine,
		endLine: selection.endLine,
		text: selection.text,
		version: version ?? null,
	}));
}

/** A whole file. */
export function fileReference(projectId: string, path: string): ContextReference {
	return { id: referenceId(), kind: "file", projectId, path, text: "" };
}

/** How a reference shows on its chip. */
export function referenceLabel(reference: ContextReference): string {
	const path = reference.path ?? "";
	const lines =
		reference.startLine !== undefined
			? linesLabel(path, reference.startLine, reference.endLine ?? reference.startLine)
			: path;
	switch (reference.kind) {
		case "lines":
			return lines;
		case "file":
			return path;
		case "diffHunk":
			return t("chatContext.references.diff", { lines });
		case "terminalOutput":
			return reference.label
				? t("chatContext.references.terminal", { label: reference.label })
				: t("chatContext.references.terminalOutput");
		case "searchResults":
			return reference.label
				? t("chatContext.references.search", { label: reference.label })
				: t("chatContext.references.searchResults");
		case "chatHistory":
			return reference.label
				? t("chatContext.references.history", { label: reference.label })
				: t("chatContext.references.chatHistory");
	}
}

/** The reference as the API takes it. */
export function toWire(reference: ContextReference): WireReference {
	return {
		kind: reference.kind,
		path: reference.path ?? null,
		startLine: reference.startLine ?? null,
		endLine: reference.endLine ?? null,
		text: reference.text,
		version: reference.version ?? null,
		label: reference.label ?? null,
	};
}

/** Files dragged from the file tree: to a folder (a move), or to a composer (context). */
export const DRAGGED_FILES = "application/x-ahab-files";

/** What a drag from the file tree carries. */
export interface DraggedFiles {
	projectId: string;
	/** Files and folders. */
	paths: string[];
	/** Those of `paths` that are folders. */
	folders?: string[];
}

/** Whether a drag carries files from the file tree (readable only on drop). */
export function carriesDraggedFiles(data: Pick<DataTransfer, "types">): boolean {
	return data.types.includes(DRAGGED_FILES);
}

/** The files a drop carries from the file tree, if any. */
export function draggedFiles(data: Pick<DataTransfer, "getData">): DraggedFiles | undefined {
	try {
		const value = JSON.parse(data.getData(DRAGGED_FILES)) as Partial<DraggedFiles> | null;
		if (typeof value?.projectId !== "string" || !Array.isArray(value.paths)) return undefined;
		const paths = value.paths.filter((path): path is string => typeof path === "string" && !!path);
		const folders = (Array.isArray(value.folders) ? value.folders : []).filter(
			(path): path is string => typeof path === "string",
		);
		return paths.length ? { projectId: value.projectId, paths, folders } : undefined;
	} catch {
		return undefined;
	}
}
