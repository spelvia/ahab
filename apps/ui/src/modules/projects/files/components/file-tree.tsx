import { cn } from "cn";
import type { TFunction } from "i18next";
import {
	ChevronRight,
	ChevronsDownUp,
	ClipboardCopy,
	ClipboardPaste,
	Columns2,
	Copy,
	CopyPlus,
	FileOutput,
	FilePlus,
	FolderPlus,
	Link,
	MessageSquarePlus,
	MessagesSquare,
	Pencil,
	RefreshCw,
	Scissors,
	Search,
	Trash2,
	ArrowDownUp,
	Check,
} from "lucide-react";
import { createContext, useContext, useEffect, useRef, useState } from "react";

import {
	useCreateFile,
	useDeleteFiles,
	useFiles,
	useRefreshListing,
	useRenameFile,
} from "@/modules/projects/files/api";
import {
	EXPAND_AFTER,
	FileDragContext,
	scrollWhileDragging,
	useFileDrag,
	useFileDragContext,
} from "@/modules/projects/files/components/file-drag";
import { announceMove, changeFileSort, useFileSort } from "@/modules/projects/files/hooks";
import {
	badName,
	folderOf,
	joinPath,
	movedPath,
	nameOf,
	rangeOf,
	topLevelPaths,
	typeAhead,
	sortEntries,
} from "@/modules/projects/files/model";
import type { FileEntry } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { useChatContext } from "@/shared/chat-context/context";
import { fileReference } from "@/shared/chat-context/model";
import { formatList, useTranslation } from "@/shared/i18n";
import { commandKey, mac } from "@/shared/platform";
import { openFileFolders, rememberOpenFileFolders } from "@/shared/preferences/store";
import { copyText } from "@/shared/ui/clipboard";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { FileIcon, FolderIcon } from "@/shared/ui/file-icon";
import { NameDialog } from "@/shared/ui/name-dialog";
import { SidebarHeading } from "@/shared/ui/sidebar-heading";
import { toast } from "@/shared/ui/toast";

type Kind = FileEntry["kind"];

/** A file or folder the tree acts on. */
interface Target {
	path: string;
	kind: Kind;
}

/** What the tree's menus and keys ask a dialog for. */
type Action =
	| { kind: "newFile" | "newFolder"; folder: string }
	| { kind: "rename"; path: string }
	| { kind: "delete"; targets: Target[] };

interface TreeProps {
	projectId: string;
	/** The file open in the focused tab, highlighted, and revealed in the tree. */
	selected?: string;
	onOpen: (path: string, keep?: boolean) => void;
	/** Opens a file beside the focused tab (its right-click menu). */
	onOpenBeside?: (path: string) => void;
	/** The project's folder on its machine, for Copy path. */
	root?: string;
	/** Searches the files of a folder (its right-click menu). */
	onFindInFolder?: (path: string) => void;
	/** A read-only project: no new, rename or delete. */
	readOnly?: boolean;
	/** A file or folder moved: its windows follow. */
	onRenamed?: (from: string, to: string) => void;
	/** A file or folder is gone: its windows close. */
	onDeleted?: (path: string) => void;
}

/** What the rows share: the tree's choice, focus, open folders and clipboard, and its commands. */
interface TreeState {
	/** The files and folders chosen, with their kinds. */
	selection: ReadonlyMap<string, Kind>;
	/** The row that takes the keys (roving focus); none before one was focused. */
	focused: string | null;
	expanded: ReadonlySet<string>;
	/** What Cut holds, shown faded until pasted. */
	cut: ReadonlySet<string>;
	canPaste: boolean;
	/** A row to scroll to once it shows (the open file, revealed). */
	revealing: string | null;
	setOpen: (path: string, open: boolean) => void;
	click: (event: React.MouseEvent<HTMLElement>, target: Target) => void;
	focus: (path: string) => void;
	reveal: (path: string) => void;
	/** What a row's menu acts on: the selection when the row is in it, else the row. */
	targets: (target: Target) => Target[];
	commands: Commands;
}

/** What the menus (and keys) do. */
interface Commands {
	create: (kind: "newFile" | "newFolder", folder: string) => void;
	rename: (path: string) => void;
	remove: (targets: Target[]) => void;
	duplicate: (targets: Target[]) => void;
	clip: (targets: Target[], cut: boolean) => void;
	paste: (folder: string) => void;
	copyPaths: (targets: Target[], absolute: boolean) => void;
}

const TreeContext = createContext<TreeState | null>(null);

function useTree(): TreeState {
	const state = useContext(TreeContext);
	if (!state) throw new Error("A tree row outside its FileTree");
	return state;
}

/** How long typed letters add up to one name to jump to, in milliseconds. */
const TYPE_AHEAD_FOR = 800;

/** The folders that hold `path`, outermost first. */
function ancestors(path: string): string[] {
	const parts = path.split("/").slice(0, -1);
	return parts.map((_, index) => parts.slice(0, index + 1).join("/"));
}

const rowsOf = (tree: HTMLElement | null): HTMLElement[] => [
	...(tree?.querySelectorAll<HTMLElement>('[role="treeitem"]') ?? []),
];

const targetOf = (row: HTMLElement): Target => ({
	path: row.dataset.path ?? "",
	kind: (row.dataset.kind ?? "file") as Kind,
});

/** The folder a new file, or what is pasted, goes into from `target`: itself if a folder. */
const folderFor = (target: Target | null | undefined): string =>
	target ? (target.kind === "dir" ? target.path : folderOf(target.path)) : "";

/**
 * A project's files, as VS Code's Explorer: folders load as they open
 * (and stay open across visits); the open file is revealed. Click chooses,
 * ⌘-click (Ctrl-click) adds, Shift-click chooses a range; the arrows,
 * Home, End and typed letters move, Enter and Space open, F2 renames,
 * Delete deletes, ⌘X ⌘C ⌘V cut, copy and paste. Right-click acts on what
 * is chosen: add to a chat, copy paths, duplicate, delete. Drag
 * onto a folder to move (⌥ to copy), or into a chat's composer.
 */
export function FileTree(props: TreeProps) {
	const { projectId, readOnly } = props;
	const writable = !readOnly;
	const refresh = useRefreshListing(projectId);
	const { t } = useTranslation("projects");
	const tree = useRef<HTMLDivElement>(null);
	const [action, setAction] = useState<Action | null>(null);
	const [selection, setSelection] = useState<ReadonlyMap<string, Kind>>(new Map());
	const [anchor, setAnchor] = useState<string | null>(null);
	const [focused, setFocused] = useState<string | null>(null);
	const [expanded, setExpanded] = useState<ReadonlySet<string>>(
		() => new Set(openFileFolders(projectId)),
	);
	const [clipboard, setClipboard] = useState<{ targets: Target[]; cut: boolean } | null>(null);
	const [revealing, setRevealing] = useState<string | null>(null);
	const typed = useRef({ text: "", at: 0 });
	/** A row to focus once a dialog has closed (and given its focus back). */
	const refocus = useRef<string | null>(null);

	useEffect(() => rememberOpenFileFolders(projectId, [...expanded]), [projectId, expanded]);

	// The file opened (here or anywhere) is revealed: its folders open, it is chosen.
	const [shown, setShown] = useState<string | undefined>();
	if (props.selected !== shown) {
		setShown(props.selected);
		const path = props.selected;
		if (path && !path.startsWith("external:")) {
			const parents = ancestors(path);
			if (parents.some((folder) => !expanded.has(folder)))
				setExpanded(new Set([...expanded, ...parents]));
			setSelection(new Map([[path, "file"]]));
			setAnchor(path);
			setFocused(path);
			setRevealing(path);
		}
	}

	useEffect(() => {
		const path = refocus.current;
		if (action || !path) return;
		refocus.current = null;
		// After the dialog gives focus back to where it was, once the row
		// shows (a renamed one comes with the next listing).
		const until = Date.now() + 1000;
		const timer = setInterval(() => {
			const row = rowsOf(tree.current).find((each) => each.dataset.path === path);
			row?.focus();
			if (row || Date.now() > until) clearInterval(timer);
		}, 50);
		return () => clearInterval(timer);
	}, [action]);

	const renamed = (from: string, to: string) => {
		const follow = (path: string) => movedPath(path, from, to);
		setSelection((now) => new Map([...now].map(([path, kind]) => [follow(path), kind])));
		setExpanded((now) => new Set([...now].map(follow)));
		setFocused((now) => now && follow(now));
		setAnchor((now) => now && follow(now));
		setClipboard(
			(now) =>
				now && {
					...now,
					targets: now.targets.map((each) => ({ ...each, path: follow(each.path) })),
				},
		);
		props.onRenamed?.(from, to);
	};
	const deleted = (path: string) => {
		const kept = (each: string) => each !== path && !each.startsWith(`${path}/`);
		setSelection((now) => new Map([...now].filter(([each]) => kept(each))));
		setExpanded((now) => new Set([...now].filter(kept)));
		setClipboard((now) => {
			const targets = now?.targets.filter((each) => kept(each.path)) ?? [];
			return now && targets.length ? { ...now, targets } : null;
		});
		props.onDeleted?.(path);
	};

	const { drag, transfer, dialog } = useFileDrag({
		projectId,
		readOnly: Boolean(readOnly),
		selection,
		onRenamed: renamed,
		onDeleted: deleted,
	});

	const rows = () => rowsOf(tree.current);
	const rowOf = (path: string) => rows().find((row) => row.dataset.path === path);

	const setOpen = (path: string, open: boolean) => {
		setExpanded((now) => {
			if (now.has(path) === open) return now;
			const next = new Set(now);
			if (open) next.add(path);
			else next.delete(path);
			return next;
		});
		// Focus inside a folder that closes goes to the folder.
		if (!open) setFocused((now) => (now?.startsWith(`${path}/`) ? path : now));
	};

	const only = (target: Target) => {
		setSelection(new Map([[target.path, target.kind]]));
		setAnchor(target.path);
	};
	/** From the anchor to `to`, in the order shown; `add` keeps what was chosen. */
	const range = (to: Target, add: boolean) => {
		const shownRows = rows().map(targetOf);
		const order = shownRows.map((each) => each.path);
		const from = anchor !== null && order.includes(anchor) ? anchor : (focused ?? to.path);
		if (from === to.path) setAnchor(to.path);
		const kinds = new Map(shownRows.map((each) => [each.path, each.kind]));
		const chosen = rangeOf(order, from, to.path).map(
			(path) => [path, kinds.get(path) ?? "file"] as const,
		);
		setSelection((now) => new Map([...(add ? now : []), ...chosen]));
	};

	const chosen = (): Target[] => [...selection].map(([path, kind]) => ({ path, kind }));

	const commands: Commands = {
		create(kind, folder) {
			if (folder) setOpen(folder, true);
			setAction({ kind, folder });
		},
		rename(path) {
			setAction({ kind: "rename", path });
		},
		remove(targets) {
			const paths = topLevelPaths(targets.map((each) => each.path));
			if (!paths.length) return;
			// Focus goes to the row after what is deleted, else the one before.
			const order = rows().map((row) => row.dataset.path ?? "");
			const gone = (path: string) =>
				paths.some((each) => path === each || path.startsWith(`${each}/`));
			const last = Math.max(...paths.map((path) => order.indexOf(path)));
			const first = Math.min(...paths.map((path) => order.indexOf(path)).filter((i) => i >= 0));
			refocus.current =
				order.slice(last + 1).find((path) => !gone(path)) ??
				order
					.slice(0, Math.max(first, 0))
					.toReversed()
					.find((path) => !gone(path)) ??
				null;
			setAction({
				kind: "delete",
				targets: targets.filter((each) => paths.includes(each.path)),
			});
		},
		duplicate(targets) {
			void transfer.duplicate(targets.map((each) => each.path));
		},
		clip(targets, cut) {
			setClipboard({ targets, cut });
		},
		paste(folder) {
			if (!clipboard || readOnly) return;
			const { targets, cut } = clipboard;
			if (folder) setOpen(folder, true);
			if (cut) setClipboard(null);
			void transfer.paste(
				targets.map((each) => each.path),
				folder,
				!cut,
			);
		},
		copyPaths(targets, absolute) {
			const base = (props.root ?? "").replace(/\/+$/, "");
			const text = targets
				.map((each) => (absolute ? `${base}/${each.path}` : each.path))
				.join("\n");
			const done =
				targets.length > 1
					? t("fileTree.copiedPaths", { count: targets.length })
					: absolute
						? t("fileTree.copiedPath")
						: t("fileTree.copiedRelativePath");
			void copyText(text).then((copied) => toast(copied ? done : t("fileTree.copyFailed")));
		},
	};

	const state: TreeState = {
		selection,
		focused,
		expanded,
		cut: new Set(clipboard?.cut ? clipboard.targets.map((each) => each.path) : []),
		canPaste: Boolean(clipboard) && writable,
		revealing,
		setOpen,
		click(event, target) {
			event.currentTarget.focus();
			if (event.shiftKey) {
				range(target, event.metaKey || event.ctrlKey);
				return;
			}
			if (event.metaKey || event.ctrlKey) {
				setSelection((now) => {
					const next = new Map(now);
					if (next.has(target.path)) next.delete(target.path);
					else next.set(target.path, target.kind);
					return next;
				});
				setAnchor(target.path);
				return;
			}
			only(target);
			if (target.kind === "dir") setOpen(target.path, !expanded.has(target.path));
			else if (target.kind === "file") props.onOpen(target.path);
		},
		focus: setFocused,
		reveal(path) {
			rowOf(path)?.scrollIntoView?.({ block: "nearest" });
			setRevealing((now) => (now === path ? null : now));
		},
		targets: (target) => (selection.has(target.path) ? chosen() : [target]),
		commands,
	};

	const keyDown = (event: React.KeyboardEvent) => {
		// Keys in a row's menu (in a portal) are the menu's.
		if (!tree.current?.contains(event.target as Node)) return;
		const all = rows();
		const row = (event.target as HTMLElement).closest<HTMLElement>('[role="treeitem"]');
		const index = row ? all.indexOf(row) : -1;
		const current = row ? targetOf(row) : null;
		const mod = event.metaKey || event.ctrlKey;
		const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
		/** What the keys act on: the selection, else the focused row. */
		const acted = (): Target[] => (selection.size ? chosen() : current ? [current] : []);
		const go = (next: HTMLElement | undefined, extend = event.shiftKey) => {
			if (!next) return;
			next.focus();
			const target = targetOf(next);
			if (extend) range(target, false);
			else only(target);
		};
		const typing = Boolean(typed.current.text) && Date.now() - typed.current.at < TYPE_AHEAD_FOR;
		const handled = (() => {
			if (mod && !event.altKey && !event.shiftKey) {
				if (key === "a")
					setSelection(new Map(all.map((each) => [targetOf(each).path, targetOf(each).kind])));
				else if (key === "c") commands.clip(acted(), false);
				else if (key === "x" && writable) commands.clip(acted(), true);
				else if (key === "v" && writable) commands.paste(folderFor(current));
				// ⌘⌫ deletes, as on a Mac.
				else if (key === "Backspace" && event.metaKey && writable) commands.remove(acted());
				else return false;
				return true;
			}
			if (event.altKey) return false;
			switch (key) {
				case "ArrowDown":
					go(index === -1 ? all[0] : all[index + 1]);
					return true;
				case "ArrowUp":
					go(index === -1 ? all[0] : all[index - 1]);
					return true;
				case "Home":
					go(all[0]);
					return true;
				case "End":
					go(all.at(-1));
					return true;
				case "ArrowRight":
					if (current?.kind !== "dir") return true;
					if (!expanded.has(current.path)) setOpen(current.path, true);
					else {
						const next = all[index + 1];
						if (next?.dataset.path?.startsWith(`${current.path}/`)) go(next, false);
					}
					return true;
				case "ArrowLeft":
					if (!current) return true;
					if (current.kind === "dir" && expanded.has(current.path)) setOpen(current.path, false);
					else if (folderOf(current.path)) go(rowOf(folderOf(current.path)), false);
					return true;
				case "Enter":
					if (!current) return true;
					if (current.kind === "dir") setOpen(current.path, !expanded.has(current.path));
					else if (current.kind === "file") props.onOpen(current.path, true);
					return true;
				case "Escape":
					setSelection(new Map());
					setClipboard(null);
					return true;
				case "F2":
					if (current && writable) commands.rename(current.path);
					return true;
				case "Delete":
					if (writable) commands.remove(acted());
					return true;
				case " ":
					if (typing) break;
					if (current?.kind === "dir") setOpen(current.path, !expanded.has(current.path));
					else if (current?.kind === "file") props.onOpen(current.path);
					return true;
				default:
					break;
			}
			// Letters typed jump to the next name that starts with them.
			if (key.length !== 1) return false;
			const now = Date.now();
			const text = typing ? typed.current.text + key : key;
			typed.current = { text, at: now };
			const found = typeAhead(
				all.map((each) => each.dataset.name ?? ""),
				text,
				index,
			);
			if (found >= 0) go(all[found], false);
			return true;
		})();
		if (handled) {
			event.preventDefault();
			event.stopPropagation();
		}
	};

	const view = (
		<div
			ref={tree}
			role="tree"
			aria-label={t("fileTree.label")}
			aria-multiselectable
			className={cn(
				"min-h-full pb-4 text-[0.8125rem]",
				drag.target === "" && "bg-primary/5 ring-primary/50 ring-1 ring-inset",
			)}
			onDragOverCapture={scrollWhileDragging}
			onDragOver={(event) => drag.over(event, "")}
			onDragLeave={(event) => {
				if (!event.currentTarget.contains(event.relatedTarget as Node | null)) drag.leave("");
			}}
			onDrop={(event) => drag.drop(event, "")}
			onKeyDown={keyDown}
		>
			<Level {...props} path="" depth={0} />
		</div>
	);
	/** Where the header's New file and New folder go: the focused folder, or the focused file's. */
	const here = () => {
		const row = focused ? rowOf(focused) : undefined;
		return folderFor(row ? targetOf(row) : null);
	};
	return (
		// oxlint-disable-next-line react/jsx-no-constructed-context-values -- every row reads the selection, new on each change anyway
		<TreeContext.Provider value={state}>
			<FileDragContext.Provider value={drag}>
				<SidebarHeading
					action={
						<div className="flex items-center gap-0.5">
							{writable ? (
								<>
									<ToolButton
										label={t("fileTree.newFile")}
										onClick={() => commands.create("newFile", here())}
									>
										<FilePlus />
									</ToolButton>
									<ToolButton
										label={t("fileTree.newFolder")}
										onClick={() => commands.create("newFolder", here())}
									>
										<FolderPlus />
									</ToolButton>
								</>
							) : null}
							<SortMenu />
							<ToolButton label={t("fileTree.refresh")} onClick={refresh}>
								<RefreshCw />
							</ToolButton>
							<ToolButton
								label={t("fileTree.collapseAll")}
								onClick={() => {
									setExpanded(new Set());
									setFocused((now) => now?.split("/")[0] ?? now);
								}}
							>
								<ChevronsDownUp />
							</ToolButton>
						</div>
					}
				>
					{t("fileTree.label")}
				</SidebarHeading>
				{writable ? (
					<ContextMenu>
						<ContextMenuTrigger asChild>{view}</ContextMenuTrigger>
						<ContextMenuContent>
							<ContextMenuItem onSelect={() => commands.create("newFile", "")}>
								<FilePlus />
								{t("fileTree.menu.newFile")}
							</ContextMenuItem>
							<ContextMenuItem onSelect={() => commands.create("newFolder", "")}>
								<FolderPlus />
								{t("fileTree.menu.newFolder")}
							</ContextMenuItem>
							{clipboard ? (
								<>
									<ContextMenuSeparator />
									<ContextMenuItem onSelect={() => commands.paste("")}>
										<ClipboardPaste />
										{t("fileTree.menu.paste")}
										<Keys>{`${commandKey}V`}</Keys>
									</ContextMenuItem>
								</>
							) : null}
						</ContextMenuContent>
					</ContextMenu>
				) : (
					view
				)}
				<Dialogs
					{...props}
					action={action}
					onRenamed={renamed}
					onDeleted={deleted}
					onDone={(result) => {
						// Cancelled: the focus goes back where it was.
						if (!result) refocus.current = null;
						else if (result.focus) refocus.current = result.focus;
						setAction(null);
					}}
				/>
				{dialog}
			</FileDragContext.Provider>
		</TreeContext.Provider>
	);
}

const SORTS = ["name", "type", "modified", "size"] as const;

/** The tree's order: by name, type, date modified or size; folders first or not. */
function SortMenu() {
	const { t } = useTranslation("projects");
	const sort = useFileSort();
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				aria-label={t("fileTree.sort.label")}
				title={t("fileTree.sort.label")}
				className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 place-items-center rounded outline-none focus-visible:ring-2 [&_svg]:size-3.5"
			>
				<ArrowDownUp />
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="w-52">
				{SORTS.map((by) => (
					<DropdownMenuItem key={by} onSelect={() => changeFileSort({ ...sort, by })}>
						<Check className={by === sort.by ? "" : "invisible"} />
						{t(`fileTree.sort.${by}`)}
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuItem
					onSelect={() => changeFileSort({ ...sort, foldersFirst: !sort.foldersFirst })}
				>
					<Check className={sort.foldersFirst ? "" : "invisible"} />
					{t("fileTree.sort.foldersFirst")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

function ToolButton({
	label,
	onClick,
	children,
}: {
	label: string;
	onClick: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			onClick={onClick}
			className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 place-items-center rounded outline-none focus-visible:ring-2 [&_svg]:size-3.5"
		>
			{children}
		</button>
	);
}

/** A menu item's keys, shown at its end; not part of its name. */
function Keys({ children }: { children: string }) {
	return (
		<span aria-hidden className="text-muted-foreground ml-auto pl-4 text-xs">
			{children}
		</span>
	);
}

/** Why a change to the files failed, in the tree's words. */
function failed(t: TFunction<"projects">, error: Error | null): string | undefined {
	return error
		? explain(error, t("fileTree.errors.failed"), {
				already_exists: t("fileTree.errors.alreadyExists"),
				read_only: t("fileTree.errors.readOnly"),
				machine_outdated: t("fileTree.errors.machineOutdated"),
			})
		: undefined;
}

/** How many names a question lists before "and N more". */
const LISTED = 5;

/** The dialogs the menus open: names for new and renamed things, a question before deleting. */
function Dialogs({
	projectId,
	action,
	onDone,
	onOpen,
	onRenamed,
	onDeleted,
}: TreeProps & {
	action: Action | null;
	/** Closed: done (with the row to focus, if the tree did not choose it) or cancelled. */
	onDone: (result?: { focus?: string }) => void;
}) {
	const { t } = useTranslation("projects");
	const create = useCreateFile(projectId);
	const rename = useRenameFile(projectId);
	const remove = useDeleteFiles(projectId);
	const [invalid, setInvalid] = useState<string>();
	// What a delete that failed part way already deleted, not asked again.
	const gone = useRef(new Set<string>());
	const close = (open: boolean, result?: { focus?: string }) => {
		if (open) return;
		setInvalid(undefined);
		create.reset();
		rename.reset();
		remove.reset();
		gone.current = new Set();
		onDone(result);
	};
	if (action?.kind === "newFile" || action?.kind === "newFolder") {
		const isFile = action.kind === "newFile";
		return (
			<NameDialog
				open
				onOpenChange={close}
				title={isFile ? t("fileTree.newFile") : t("fileTree.newFolder")}
				label={
					action.folder
						? t("fileTree.create.nameIn", { folder: action.folder })
						: t("fileTree.create.name")
				}
				action={t("fileTree.create.action")}
				pending={create.isPending}
				error={invalid ?? failed(t, create.error)}
				onSubmit={(name) => {
					const bad = badName(name);
					setInvalid(bad);
					if (bad) return;
					const path = joinPath(action.folder, name);
					create.mutate(
						{ id: projectId, data: { path, kind: isFile ? "file" : "folder" } },
						{
							onSuccess: () => {
								close(false, { focus: isFile ? undefined : path });
								if (isFile) onOpen(path, true);
							},
						},
					);
				}}
			/>
		);
	}
	if (action?.kind === "rename") {
		const { path } = action;
		return (
			<NameDialog
				open
				onOpenChange={close}
				title={t("fileTree.rename.title", { name: nameOf(path) })}
				label={t("fileTree.rename.label")}
				initial={path}
				action={t("fileTree.rename.action")}
				pending={rename.isPending}
				error={invalid ?? failed(t, rename.error)}
				onSubmit={(name) => {
					const bad = badName(name);
					setInvalid(bad);
					if (bad || name.trim() === path) return;
					rename.mutate(
						{ id: projectId, data: { from: path, to: name.trim() } },
						{
							onSuccess: (moved) => {
								close(false, { focus: moved.path });
								announceMove(projectId, path, moved.path);
								onRenamed?.(path, moved.path);
							},
						},
					);
				}}
			/>
		);
	}
	if (action?.kind === "delete") {
		const { targets } = action;
		const [one] = targets;
		const names = targets.map((each) => nameOf(each.path));
		const more = names.length - LISTED;
		return (
			<ConfirmDialog
				open
				onOpenChange={close}
				title={
					targets.length === 1
						? t("fileTree.delete.title", { name: names[0] })
						: t("fileTree.delete.manyTitle", { count: targets.length })
				}
				description={
					targets.length === 1 && one
						? one.kind === "dir"
							? t("fileTree.delete.folder", { path: one.path })
							: t("fileTree.delete.file", { path: one.path })
						: more > 0
							? t("fileTree.delete.manyMore", {
									names: formatList(names.slice(0, LISTED), "unit"),
									count: more,
								})
							: t("fileTree.delete.many", { names: formatList(names) })
				}
				action={t("actions.delete", { ns: "common" })}
				destructive
				pending={remove.isPending}
				error={failed(t, remove.error)}
				onConfirm={() =>
					remove.mutate(
						{
							paths: targets.map((each) => each.path).filter((path) => !gone.current.has(path)),
							onEach: (path) => {
								gone.current.add(path);
								onDeleted?.(path);
							},
						},
						// The tree chose where the focus goes.
						{ onSuccess: () => close(false, {}) },
					)
				}
			/>
		);
	}
	return null;
}

function Level(props: TreeProps & { path: string; depth: number }) {
	const { projectId, path, depth } = props;
	const { t } = useTranslation("projects");
	const listing = useFiles(projectId, path);
	const sort = useFileSort();
	if (listing.isPending) return <Note depth={depth}>{t("state.loading", { ns: "common" })}</Note>;
	if (listing.isError)
		return <Note depth={depth}>{explain(listing.error, t("fileTree.cannotList"))}</Note>;
	const { truncated } = listing.data;
	const entries = sortEntries(listing.data.entries, sort);
	if (entries.length === 0 && depth === 0) return <Note depth={depth}>{t("fileTree.empty")}</Note>;
	return (
		<>
			{entries.map((entry, index) =>
				entry.kind === "dir" ? (
					<FolderNode
						key={entry.path}
						{...props}
						entry={entry}
						depth={depth}
						first={depth === 0 && index === 0}
					/>
				) : (
					<FileNode
						key={entry.path}
						{...props}
						entry={entry}
						depth={depth}
						first={depth === 0 && index === 0}
					/>
				),
			)}
			{truncated ? <Note depth={depth}>{t("fileTree.truncated")}</Note> : null}
		</>
	);
}

type NodeProps = TreeProps & { entry: FileEntry; depth: number; first: boolean };

function FileNode(props: NodeProps) {
	const { entry, depth, first, onOpen } = props;
	const { t } = useTranslation("projects");
	const drag = useFileDragContext();
	const folder = folderOf(entry.path);
	const row = (
		<Row
			target={{ path: entry.path, kind: entry.kind }}
			name={entry.name}
			depth={depth}
			first={first}
			current={props.selected === entry.path}
			dim={entry.hidden || entry.kind === "other"}
			onDoubleClick={() => entry.kind === "file" && onOpen(entry.path, true)}
			drag={
				drag
					? {
							draggable: true,
							onDragStart: (event) => drag.start(event, entry.path, false),
							onDragEnd: drag.end,
							// Onto a file: into its folder.
							onDragOver: (event) => drag.over(event, folder),
							onDrop: (event) => drag.drop(event, folder),
						}
					: undefined
			}
			title={entry.kind === "other" ? t("fileTree.outsideLink") : entry.path}
		>
			<FileIcon path={entry.name} className="size-3.5" />
			<span className="truncate">{entry.name}</span>
		</Row>
	);
	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
			<EntryMenu {...props} />
		</ContextMenu>
	);
}

function FolderNode(props: NodeProps) {
	const { entry, depth, first } = props;
	const tree = useTree();
	const open = tree.expanded.has(entry.path);
	const drag = useFileDragContext();
	// Hovering a closed folder while dragging opens it.
	const expand = useRef<ReturnType<typeof setTimeout>>(undefined);
	const stopExpanding = () => {
		clearTimeout(expand.current);
		expand.current = undefined;
	};
	useEffect(() => {
		const timer = expand;
		return () => clearTimeout(timer.current);
	}, []);
	const row = (
		<Row
			target={{ path: entry.path, kind: "dir" }}
			name={entry.name}
			depth={depth}
			first={first}
			dim={entry.hidden}
			expanded={open}
			dropTarget={drag?.target === entry.path}
			title={entry.path}
			drag={
				drag
					? {
							draggable: true,
							onDragStart: (event) => drag.start(event, entry.path, true),
							onDragEnd: drag.end,
							onDragOver: (event) => {
								const taken = drag.over(event, entry.path);
								if (taken && !open && !expand.current)
									expand.current = setTimeout(() => {
										expand.current = undefined;
										tree.setOpen(entry.path, true);
									}, EXPAND_AFTER);
								if (!taken) stopExpanding();
							},
							onDragLeave: () => {
								stopExpanding();
								drag.leave(entry.path);
							},
							onDrop: (event) => {
								stopExpanding();
								drag.drop(event, entry.path);
							},
						}
					: undefined
			}
		>
			<ChevronRight
				className={cn(
					"text-muted-foreground size-3.5 shrink-0 transition-transform duration-100",
					open && "rotate-90",
				)}
			/>
			<FolderIcon name={entry.name} open={open} className="size-3.5" />
			<span className="truncate">{entry.name}</span>
		</Row>
	);
	return (
		<>
			<ContextMenu>
				<ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
				<EntryMenu {...props} />
			</ContextMenu>
			{open ? <Level {...props} path={entry.path} depth={depth + 1} /> : null}
		</>
	);
}

/**
 * A row's right-click menu. On a row that is chosen with others it acts on
 * all of them (add to a chat, cut, copy, copy paths, duplicate, delete);
 * what only fits one (open, new, find, rename) shows for one alone.
 */
function EntryMenu({
	entry,
	projectId,
	readOnly,
	onOpen,
	onOpenBeside,
	onFindInFolder,
	root,
}: NodeProps) {
	const { t } = useTranslation("projects");
	const chat = useChatContext();
	const tree = useTree();
	const { commands } = tree;
	const self: Target = { path: entry.path, kind: entry.kind };
	const targets = tree.targets(self);
	const many = targets.length > 1;
	const isFolder = entry.kind === "dir";
	const files = targets.filter((each) => each.kind === "file").map((each) => each.path);
	const references = () => files.map((path) => fileReference(projectId, path));
	const into = folderFor(self);

	const groups: React.ReactNode[][] = [
		chat && files.length
			? [
					chat.target ? (
						<ContextMenuItem key="add" onSelect={() => chat.add(references())}>
							<MessageSquarePlus />
							{many
								? t("fileTree.menu.addFilesToChat", {
										count: files.length,
										title: chat.target.title,
									})
								: t("fileTree.menu.addToChat", { title: chat.target.title })}
						</ContextMenuItem>
					) : null,
					<ContextMenuItem key="choose" onSelect={() => chat.choose(references())}>
						<MessagesSquare />
						{many
							? t("fileTree.menu.chooseChatFiles", { count: files.length })
							: t("fileTree.menu.chooseChat")}
					</ContextMenuItem>,
				]
			: [],
		!many && entry.kind === "file"
			? [
					<ContextMenuItem key="open" onSelect={() => onOpen(entry.path, true)}>
						<FileOutput />
						{t("fileTree.menu.open")}
					</ContextMenuItem>,
					onOpenBeside ? (
						<ContextMenuItem key="beside" onSelect={() => onOpenBeside(entry.path)}>
							<Columns2 />
							{t("fileTree.menu.openBeside")}
						</ContextMenuItem>
					) : null,
				]
			: [],
		many
			? []
			: [
					readOnly ? null : (
						<ContextMenuItem key="newFile" onSelect={() => commands.create("newFile", into)}>
							<FilePlus />
							{t("fileTree.menu.newFile")}
						</ContextMenuItem>
					),
					readOnly ? null : (
						<ContextMenuItem key="newFolder" onSelect={() => commands.create("newFolder", into)}>
							<FolderPlus />
							{t("fileTree.menu.newFolder")}
						</ContextMenuItem>
					),
					isFolder && onFindInFolder ? (
						<ContextMenuItem key="find" onSelect={() => onFindInFolder(entry.path)}>
							<Search />
							{t("fileTree.menu.findInFolder")}
						</ContextMenuItem>
					) : null,
				],
		readOnly
			? []
			: [
					<ContextMenuItem key="cut" onSelect={() => commands.clip(targets, true)}>
						<Scissors />
						{t("fileTree.menu.cut")}
						<Keys>{`${commandKey}X`}</Keys>
					</ContextMenuItem>,
					<ContextMenuItem key="copy" onSelect={() => commands.clip(targets, false)}>
						<ClipboardCopy />
						{t("fileTree.menu.copy")}
						<Keys>{`${commandKey}C`}</Keys>
					</ContextMenuItem>,
					tree.canPaste ? (
						<ContextMenuItem key="paste" onSelect={() => commands.paste(into)}>
							<ClipboardPaste />
							{t("fileTree.menu.paste")}
							<Keys>{`${commandKey}V`}</Keys>
						</ContextMenuItem>
					) : null,
				],
		[
			root ? (
				<ContextMenuItem key="path" onSelect={() => commands.copyPaths(targets, true)}>
					<Copy />
					{many ? t("fileTree.menu.copyPaths") : t("fileTree.menu.copyPath")}
				</ContextMenuItem>
			) : null,
			<ContextMenuItem key="relative" onSelect={() => commands.copyPaths(targets, false)}>
				<Link />
				{many ? t("fileTree.menu.copyRelativePaths") : t("fileTree.menu.copyRelativePath")}
			</ContextMenuItem>,
		],
		readOnly
			? []
			: [
					many ? null : (
						<ContextMenuItem key="rename" onSelect={() => commands.rename(entry.path)}>
							<Pencil />
							{t("fileTree.menu.rename")}
							<Keys>F2</Keys>
						</ContextMenuItem>
					),
					<ContextMenuItem key="duplicate" onSelect={() => commands.duplicate(targets)}>
						<CopyPlus />
						{t("fileTree.menu.duplicate")}
					</ContextMenuItem>,
					<ContextMenuItem key="delete" onSelect={() => commands.remove(targets)}>
						<Trash2 />
						{many
							? t("fileTree.menu.deleteMany", {
									count: topLevelPaths(targets.map((each) => each.path)).length,
								})
							: t("fileTree.menu.delete")}
						<Keys>{mac ? "⌘⌫" : "Del"}</Keys>
					</ContextMenuItem>,
				],
	];
	// The groups shown, a line between each two.
	const items: React.ReactNode[] = [];
	for (const group of groups) {
		const shown = group.filter(Boolean);
		if (!shown.length) continue;
		if (items.length) items.push(<ContextMenuSeparator key={`after-${items.length}`} />);
		items.push(...shown);
	}
	return (
		<ContextMenuContent onClick={(event) => event.stopPropagation()}>{items}</ContextMenuContent>
	);
}

function Row({
	target,
	name,
	depth,
	first,
	current,
	dropTarget,
	dim,
	expanded,
	onDoubleClick,
	title,
	drag,
	children,
	...trigger
}: Omit<React.ComponentProps<"div">, "onDoubleClick" | "title" | "children"> & {
	target: Target;
	name: string;
	depth: number;
	/** The tree's first row, which takes the focus before any other did. */
	first: boolean;
	/** The file open in the focused tab. */
	current?: boolean;
	/** Where a drag would drop now. */
	dropTarget?: boolean;
	dim?: boolean;
	expanded?: boolean;
	onDoubleClick?: () => void;
	title?: string;
	drag?: Pick<
		React.HTMLAttributes<HTMLDivElement>,
		"draggable" | "onDragStart" | "onDragEnd" | "onDragOver" | "onDragLeave" | "onDrop"
	>;
	children: React.ReactNode;
}) {
	const tree = useTree();
	const { path } = target;
	const chosen = tree.selection.has(path);
	const focusable = tree.focused === null ? first : tree.focused === path;
	const { revealing, reveal } = tree;
	useEffect(() => {
		if (revealing === path) reveal(path);
	}, [revealing, path, reveal]);
	return (
		<div
			// A context menu's trigger (asChild) gives its handlers here.
			{...trigger}
			role="treeitem"
			aria-selected={chosen}
			aria-expanded={expanded}
			aria-level={depth + 1}
			tabIndex={focusable ? 0 : -1}
			data-path={path}
			data-kind={target.kind}
			data-name={name}
			onFocus={() => tree.focus(path)}
			onClick={(event) => tree.click(event, target)}
			onDoubleClick={onDoubleClick}
			title={title}
			{...drag}
			style={{ paddingLeft: `${0.75 + depth * 0.875}rem` }}
			className={cn(
				"focus-visible:ring-ring flex h-[1.625rem] w-full cursor-default items-center gap-1.5 pr-3 text-left outline-none select-none focus-visible:ring-1 focus-visible:ring-inset",
				chosen
					? current
						? "bg-primary/25 text-heading"
						: "bg-primary/15 text-heading"
					: current
						? "bg-accent text-heading"
						: "hover:bg-accent/60",
				dropTarget && "bg-primary/20 ring-primary/60 ring-1 ring-inset",
				dim && !current && !chosen && "text-muted-foreground",
				tree.cut.has(path) && "opacity-55",
			)}
		>
			{children}
		</div>
	);
}

function Note({ depth, children }: { depth: number; children: React.ReactNode }) {
	return (
		<p
			className="text-muted-foreground py-1 pr-3 text-xs"
			style={{ paddingLeft: `${1.75 + depth * 0.875}rem` }}
		>
			{children}
		</p>
	);
}
