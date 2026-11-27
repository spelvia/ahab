import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useEffectEvent,
	useRef,
	useState,
} from "react";

import { deleteEntry, namesIn, transferFile, useRefreshFiles } from "@/modules/projects/files/api";
import { announceMove } from "@/modules/projects/files/hooks";
import {
	folderOf,
	joinPath,
	keepBothName,
	type Move,
	nameOf,
	planMoves,
	topLevelPaths,
} from "@/modules/projects/files/model";
import { problemCode } from "@/shared/api/fetch";
import type { FileEntry } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { DRAGGED_FILES, type DraggedFiles } from "@/shared/chat-context/model";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { toast } from "@/shared/ui/toast";

/** How long ⌘Z undoes the last move, in milliseconds. */
const UNDO_FOR = 60_000;
/** How long hovering a closed folder while dragging opens it. */
export const EXPAND_AFTER = 600;

/** What a drop did, to undo it. */
interface Done extends Move {
	copy: boolean;
	/** Something was at `to` and is gone. */
	replaced: boolean;
}

type Choice = "replace" | "keepBoth" | "cancel";

/** The drag this tree started, while it lasts (a drop reads its data only at the end). */
let dragging: DraggedFiles | null = null;

/** What the tree's rows need to drag and take drops. */
export interface FileDrag {
	/** The paths a drag of `path` carries: the selection when it holds it. */
	start: (event: React.DragEvent, path: string, folder: boolean) => void;
	end: () => void;
	/** Whether dropping here (onto `folder`, "" for the root) is taken; marks it. */
	over: (event: React.DragEvent, folder: string) => boolean;
	leave: (folder: string) => void;
	drop: (event: React.DragEvent, folder: string) => void;
	/** The folder a drop would go into now, if any. */
	target: string | null;
}

/** Moving and copying from the tree's menus and keys, as a drop does it. */
export interface FileTransfer {
	/** Pastes `paths` into `folder`, moved (cut) or copied; a copy into its own folder goes beside it. */
	paste: (paths: string[], folder: string, copy: boolean) => Promise<void>;
	/** Copies each of `paths` beside itself, named as Keep both names it. */
	duplicate: (paths: string[]) => Promise<void>;
}

export const FileDragContext = createContext<FileDrag | null>(null);

export function useFileDragContext(): FileDrag | null {
	return useContext(FileDragContext);
}

/**
 * Dragging files and folders in a project's tree: onto a folder (or the
 * tree's empty space, for the project's own folder) moves them there,
 * with ⌥ (Alt) held copies them. A name already there asks Replace, Keep
 * both or Cancel. A read-only project takes no drops. Open windows follow
 * what moved; the toast and ⌘Z undo it. Cut, Copy, Paste and Duplicate
 * (`transfer`) work the same way.
 */
export function useFileDrag({
	projectId,
	readOnly,
	selection,
	onRenamed,
	onDeleted,
}: {
	projectId: string;
	readOnly: boolean;
	/** The files and folders chosen in the tree, dragged together, with their kinds. */
	selection: ReadonlyMap<string, FileEntry["kind"]>;
	onRenamed?: (from: string, to: string) => void;
	onDeleted?: (path: string) => void;
}): { drag: FileDrag; transfer: FileTransfer; dialog: React.ReactNode } {
	const { t } = useTranslation("projects");
	const refresh = useRefreshFiles(projectId);
	const [target, setTarget] = useState<string | null>(null);
	const [conflict, setConflict] = useState<{
		name: string;
		folder: string;
		copy: boolean;
		answer: (choice: Choice) => void;
	} | null>(null);
	const last = useRef<{ done: Done[]; at: number } | null>(null);

	const ask = (move: Move, copy: boolean) =>
		new Promise<Choice>((resolve) =>
			setConflict({
				name: nameOf(move.to),
				folder: folderOf(move.to),
				copy,
				answer: (choice) => {
					setConflict(null);
					resolve(choice);
				},
			}),
		);

	const followed = (from: string, to: string) => {
		announceMove(projectId, from, to);
		onRenamed?.(from, to);
	};

	const undo = async (done: Done[]) => {
		last.current = null;
		try {
			for (const step of done.toReversed()) {
				if (step.copy) {
					// oxlint-disable-next-line no-await-in-loop -- one at a time, last first
					await deleteEntry(projectId, step.to);
					onDeleted?.(step.to);
				} else {
					// oxlint-disable-next-line no-await-in-loop
					await transferFile(projectId, { from: step.to, to: step.from });
					followed(step.to, step.from);
				}
			}
			toast(done.some((step) => step.copy) ? t("fileDrag.copyUndone") : t("fileDrag.moveUndone"));
		} catch (error) {
			toast(explain(error, t("fileDrag.cannotUndo")), { duration: 8000 });
		} finally {
			refresh();
		}
	};

	/**
	 * Moves (or copies) each, asking on a clash; the toast (`said`, told
	 * whether something was replaced) offers Undo.
	 */
	const run = async (
		moves: Move[],
		copy: boolean,
		said: (done: Done[], replacing: boolean) => string,
	) => {
		const done: Done[] = [];
		let failed: string | undefined;
		steps: for (const move of moves) {
			let next = move;
			let replace = false;
			for (;;) {
				try {
					// oxlint-disable-next-line no-await-in-loop -- each may ask first
					const result = await transferFile(projectId, next, { copy, replace });
					done.push({ ...next, to: result.path, copy, replaced: replace });
					if (!copy) followed(next.from, result.path);
					break;
				} catch (error) {
					if (problemCode(error) !== "already_exists" || replace) {
						failed = explain(error, t("fileDrag.failed"), {
							machine_outdated: t("fileDrag.machineOutdated"),
						});
						break steps;
					}
					// oxlint-disable-next-line no-await-in-loop
					const choice = await ask(next, copy);
					if (choice === "cancel") break steps;
					if (choice === "replace") replace = true;
					else {
						const there = folderOf(next.to);
						// oxlint-disable-next-line no-await-in-loop
						const names = await namesIn(projectId, there).catch(() => new Set<string>());
						next = {
							from: next.from,
							to: joinPath(
								there,
								keepBothName(nameOf(next.to), (name) => names.has(name)),
							),
						};
					}
				}
			}
		}
		refresh();
		if (failed) toast(failed, { duration: 8000 });
		if (!done.length) return;
		if (done.some((step) => step.replaced)) {
			// What was replaced is gone: there is nothing to undo it to.
			last.current = null;
			toast(said(done, true));
			return;
		}
		last.current = { done, at: Date.now() };
		toast(said(done, false), {
			action: { label: t("actions.undo", { ns: "common" }), run: () => void undo(done) },
			duration: 8000,
		});
	};

	/** Each of `paths` copied beside itself, under a free name. */
	const besides = async (paths: string[]): Promise<Move[]> => {
		const folders = [...new Set(paths.map(folderOf))];
		const listed = await Promise.all(
			folders.map((folder) => namesIn(projectId, folder).catch(() => new Set<string>())),
		);
		const taken = new Map(folders.map((folder, index) => [folder, listed[index] ?? new Set()]));
		return paths.map((from) => {
			const names = taken.get(folderOf(from)) ?? new Set<string>();
			const name = keepBothName(nameOf(from), (each) => names.has(each));
			names.add(name);
			return { from, to: joinPath(folderOf(from), name) };
		});
	};

	/**
	 * Moves or copies into `folder`, as a drop there does; with `beside`
	 * (pasting), a copy into its own folder is made beside it.
	 */
	const into = async (paths: string[], folder: string, copy: boolean, beside = false) => {
		const { moves, refused } = planMoves(paths, folder);
		if (refused) {
			toast(refused);
			return;
		}
		// A copy into its own folder goes beside it; a move there does nothing.
		const here =
			copy && beside ? topLevelPaths(paths).filter((path) => folderOf(path) === folder) : [];
		const all = here.length ? [...moves, ...(await besides(here))] : moves;
		if (!all.length) return;
		await run(all, copy, (done, replacing) => {
			const said = { count: done.length, name: nameOf(done[0]?.from ?? ""), folder };
			if (replacing)
				return copy
					? folder
						? t("fileDrag.doneReplacing.copied.toFolder", said)
						: t("fileDrag.doneReplacing.copied.toRoot", said)
					: folder
						? t("fileDrag.doneReplacing.moved.toFolder", said)
						: t("fileDrag.doneReplacing.moved.toRoot", said);
			return copy
				? folder
					? t("fileDrag.done.copied.toFolder", said)
					: t("fileDrag.done.copied.toRoot", said)
				: folder
					? t("fileDrag.done.moved.toFolder", said)
					: t("fileDrag.done.moved.toRoot", said);
		});
	};

	const duplicate = async (paths: string[]) => {
		const moves = await besides(topLevelPaths(paths));
		await run(moves, true, (done) =>
			t("fileDrag.duplicated", { count: done.length, name: nameOf(done[0]?.to ?? "") }),
		);
	};

	const end = useCallback(() => {
		dragging = null;
		setTarget(null);
	}, []);

	// A drag that ends anywhere (Escape, outside the window) leaves no mark.
	useEffect(() => {
		window.addEventListener("dragend", end);
		window.addEventListener("drop", end);
		return () => {
			window.removeEventListener("dragend", end);
			window.removeEventListener("drop", end);
		};
	}, [end]);

	// ⌘Z (Ctrl+Z) soon after a drop undoes it, wherever the focus is but in
	// text, which has its own undo.
	const undoLast = useEffectEvent((event: KeyboardEvent) => {
		const recent = last.current;
		if (!recent || Date.now() - recent.at > UNDO_FOR) return;
		if (!(event.metaKey || event.ctrlKey) || event.shiftKey || event.key.toLowerCase() !== "z")
			return;
		const focused = event.target as HTMLElement | null;
		if (focused?.closest("input, textarea, [contenteditable=true], .cm-editor")) return;
		event.preventDefault();
		void undo(recent.done);
	});
	useEffect(() => {
		const listener = (event: KeyboardEvent) => undoLast(event);
		window.addEventListener("keydown", listener);
		return () => window.removeEventListener("keydown", listener);
	}, []);

	const drag: FileDrag = {
		start(event, path, folder) {
			const chosen = selection.has(path)
				? [...selection].map(([each, kind]) => [each, kind === "dir"] as const)
				: [[path, folder] as const];
			const paths = chosen.map(([each]) => each);
			const folders = chosen.filter(([, isFolder]) => isFolder).map(([each]) => each);
			dragging = { projectId, paths, folders };
			event.dataTransfer.setData(DRAGGED_FILES, JSON.stringify(dragging));
			event.dataTransfer.setData("text/plain", paths.join("\n"));
			event.dataTransfer.effectAllowed = readOnly ? "copy" : "copyMove";
		},
		end,
		over(event, folder) {
			const ours = dragging?.projectId === projectId ? dragging : null;
			if (!ours) return false;
			event.preventDefault();
			event.stopPropagation();
			const { moves, refused } = planMoves(ours.paths, folder);
			if (readOnly || refused || !moves.length) {
				event.dataTransfer.dropEffect = "none";
				setTarget(null);
				return false;
			}
			event.dataTransfer.dropEffect = event.altKey ? "copy" : "move";
			setTarget(folder);
			return true;
		},
		leave(folder) {
			setTarget((now) => (now === folder ? null : now));
		},
		drop(event, folder) {
			const ours = dragging?.projectId === projectId ? dragging : null;
			event.preventDefault();
			event.stopPropagation();
			end();
			if (!ours || readOnly) return;
			void into(ours.paths, folder, event.altKey);
		},
		target,
	};

	const dialog = conflict ? (
		<Dialog open onOpenChange={(open) => !open && conflict.answer("cancel")}>
			<DialogContent size="md" role="alertdialog">
				<DialogHeader className="pb-6">
					<DialogTitle>{t("fileDrag.conflict.title", { name: conflict.name })}</DialogTitle>
					<DialogDescription>
						{conflict.copy
							? conflict.folder
								? t("fileDrag.conflict.copy.inFolder", conflict)
								: t("fileDrag.conflict.copy.inRoot", conflict)
							: conflict.folder
								? t("fileDrag.conflict.move.inFolder", conflict)
								: t("fileDrag.conflict.move.inRoot", conflict)}
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button variant="ghost" onClick={() => conflict.answer("cancel")}>
						{t("actions.cancel", { ns: "common" })}
					</Button>
					<Button variant="outline" onClick={() => conflict.answer("keepBoth")}>
						{t("fileDrag.conflict.keepBoth")}
					</Button>
					<Button variant="destructive" onClick={() => conflict.answer("replace")}>
						{t("fileDrag.conflict.replace")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	) : null;

	return {
		drag,
		transfer: { paste: (paths, folder, copy) => into(paths, folder, copy, true), duplicate },
		dialog,
	};
}

/** The nearest ancestor that scrolls, for scrolling while dragging near its edges. */
function scrollParent(element: HTMLElement | null): HTMLElement | null {
	for (let node = element; node; node = node.parentElement) {
		const { overflowY } = getComputedStyle(node);
		if ((overflowY === "auto" || overflowY === "scroll") && node.scrollHeight > node.clientHeight)
			return node;
	}
	return null;
}

/** Scrolls the tree while a drag hovers near its top or bottom edge. */
export function scrollWhileDragging(event: React.DragEvent) {
	const area = scrollParent(event.currentTarget as HTMLElement);
	if (!area) return;
	const box = area.getBoundingClientRect();
	const edge = 32;
	if (event.clientY < box.top + edge) area.scrollBy({ top: -14 });
	else if (event.clientY > box.bottom - edge) area.scrollBy({ top: 14 });
}
