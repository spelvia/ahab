import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { editorConfig, type Move } from "@/modules/projects/files/model";
import {
	deleteFile,
	getListFilesQueryKey,
	listFiles,
	moveFile,
	readFile,
	renameFile,
	saveFile,
	useCreateFile as useGeneratedCreateFile,
	useHomeFolder as useGeneratedHomeFolder,
	useListFiles,
	useListFolders,
	useReadFile,
	useRenameFile as useGeneratedRenameFile,
	useSaveFile as useGeneratedSaveFile,
} from "@/shared/api/generated/projects-files/projects-files";
import { useInvalidate } from "@/shared/api/invalidate";
import { isExternal, loadExternalFile } from "@/shared/editor/registry";
import { t } from "@/shared/i18n";

/** A folder's entries in a project. */
export function useFiles(projectId: string, path: string) {
	return useListFiles(projectId, { path }, { query: { retry: false, staleTime: 5_000 } });
}

/**
 * A file's content in a project, with its version; a large file's first
 * part. Its window follows changes on disk itself (useFileWatch), so it is
 * not asked again on focus.
 */
export function useFile(projectId: string, path: string, enabled = true) {
	return useReadFile(
		projectId,
		{ path, partial: true },
		{ query: { enabled, retry: false, refetchOnWindowFocus: false } },
	);
}

/** Saves a file over the version its edit started from. */
export function useSaveFile() {
	return useGeneratedSaveFile();
}

/** Refresh the root and open folders; collapsed folders refetch when opened. */
export function useRefreshListing(projectId: string) {
	const client = useQueryClient();
	return () => void client.invalidateQueries({ queryKey: getListFilesQueryKey(projectId) });
}

/** The folder listings and files a change can affect. */
export function useRefreshFiles(projectId: string) {
	return useInvalidate(`/projects/${projectId}/files`, `/projects/${projectId}/file`);
}

export function useCreateFile(projectId: string) {
	return useGeneratedCreateFile({ mutation: { onSuccess: useRefreshFiles(projectId) } });
}

export function useRenameFile(projectId: string) {
	return useGeneratedRenameFile({ mutation: { onSuccess: useRefreshFiles(projectId) } });
}

/**
 * Deletes files and folders one after another, stopping at the first that
 * fails; `onEach` hears of each one gone, so a retry can skip it.
 */
export function useDeleteFiles(projectId: string) {
	const refresh = useRefreshFiles(projectId);
	return useMutation({
		mutationFn: async ({ paths, onEach }: { paths: string[]; onEach?: (path: string) => void }) => {
			for (const path of paths) {
				// oxlint-disable-next-line no-await-in-loop -- one at a time, stopping at a failure
				await deleteFile(projectId, { path });
				onEach?.(path);
			}
		},
		onSettled: () => refresh(),
	});
}

/**
 * Moves or copies a file or folder (dragging it in the tree); over what is
 * there only with `replace`. A plain move is a rename, which every daemon
 * can do; copying and replacing need one with `fs.move`.
 */
export function transferFile(
	projectId: string,
	{ from, to }: Move,
	{ copy = false, replace = false }: { copy?: boolean; replace?: boolean } = {},
) {
	return copy || replace
		? moveFile(projectId, { from, to, copy, replace })
		: renameFile(projectId, { from, to });
}

/** Deletes a file or folder (undoing a copy). */
export function deleteEntry(projectId: string, path: string) {
	return deleteFile(projectId, { path });
}

/** The names in a folder, to find one free for Keep both. */
export async function namesIn(projectId: string, folder: string): Promise<Set<string>> {
	const listing = await listFiles(projectId, { path: folder });
	return new Set(listing.entries.map((entry) => entry.name));
}

/** Where the folder picker starts on a machine. */
export function useHomeFolder(machine: string | undefined) {
	return useGeneratedHomeFolder(machine ?? "", {
		query: { enabled: Boolean(machine), retry: false },
	});
}

/** The subfolders of a folder on a machine. */
export function useFolders(machine: string | undefined, path: string | undefined, hidden: boolean) {
	return useListFolders(
		machine ?? "",
		{ path: path ?? "", hidden },
		{ query: { enabled: Boolean(machine && path), retry: false } },
	);
}

/**
 * Changes a file on disk as a person would: read
 * it, change it, save it over the version read. For edits from tools to
 * files no window has open.
 */
export async function editProjectFile(
	projectId: string,
	path: string,
	change: (text: string) => string,
): Promise<void> {
	const file = await readFile(projectId, { path });
	if (file.binary || file.truncated)
		throw new Error(t("filesApi.cannotEdit", { ns: "projects", path }));
	await saveFile(
		projectId,
		{ content: change(file.content ?? ""), expectedVersion: file.version ?? undefined },
		{ path },
	);
}

/**
 * A file outside the project a language server pointed at,
 * read through its grant; kept while its window is open.
 */
export function useExternalFile(projectId: string, path: string) {
	return useQuery({
		queryKey: ["external-file", projectId, path],
		queryFn: () => loadExternalFile(projectId, path),
		enabled: isExternal(path),
		retry: false,
		staleTime: Infinity,
		refetchOnWindowFocus: false,
	});
}

/** What `.editorconfig` files say of a file's indentation. */
export function useEditorConfig(projectId: string, path: string, enabled = true) {
	return useQuery({
		queryKey: ["editorconfig", projectId, path],
		enabled,
		staleTime: 60_000,
		retry: false,
		queryFn: async () => {
			const parts = path.split("/").filter(Boolean);
			const folders = parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join("/"));
			const nearestFirst = ["", ...folders].toReversed();
			const read = await Promise.all(
				nearestFirst.map((folder) =>
					readFile(projectId, { path: folder ? `${folder}/.editorconfig` : ".editorconfig" })
						.then((file) => ({
							text: file.content ?? "",
							path: folder ? path.slice(folder.length + 1) : path,
						}))
						.catch(() => null),
				),
			);
			return editorConfig(read.filter((file) => file !== null));
		},
	});
}

/** Locate an unnumbered post-edit hunk in the current file when it has one exact match. */
export async function locateEdit(project: string, path: string, lines: string[]) {
	const file = await readFile(project, { path, partial: true });
	if (file.binary || file.truncated || !file.content || !lines.some((line) => line.trim()))
		return undefined;
	const source = "\n" + file.content.replaceAll("\r\n", "\n") + "\n";
	const fragment = "\n" + lines.join("\n") + "\n";
	const at = source.indexOf(fragment);
	if (at < 0 || source.indexOf(fragment, at + 1) >= 0) return undefined;
	return source.slice(0, at + 1).split("\n").length - 1;
}
