import { diffLines, diffWordsWithSpace } from "diff";

/** A piece of a changed line: changed or not. */
interface Piece {
	text: string;
	changed: boolean;
}

/** One side of a row: its line number and text (with its changed pieces). */
export interface Cell {
	line: number;
	text: string;
	pieces?: Piece[];
}

/** A row of a side-by-side diff. */
export type Row =
	| { kind: "same" | "change"; left: Cell; right: Cell }
	| { kind: "remove"; left: Cell }
	| { kind: "add"; right: Cell }
	/** Unchanged lines folded away; `at` names the fold. */
	| { kind: "fold"; at: number; hidden: number };

function lines(text: string): string[] {
	const all = text.split("\n");
	if (all.at(-1) === "") all.pop();
	return all;
}

/** The changed and unchanged pieces of a line, on one side. */
function pieces(before: string, after: string): [Piece[], Piece[]] {
	const left: Piece[] = [];
	const right: Piece[] = [];
	for (const part of diffWordsWithSpace(before, after)) {
		if (!part.added) left.push({ text: part.value, changed: Boolean(part.removed) });
		if (!part.removed) right.push({ text: part.value, changed: Boolean(part.added) });
	}
	return [left, right];
}

/**
 * Two versions of a file as rows side by side: unchanged lines face each
 * other, removed lines face added ones (with the changed words marked),
 * the rest stand alone.
 */
export function sideBySide(before: string, after: string): Row[] {
	const rows: Row[] = [];
	let left = 1;
	let right = 1;
	const parts = diffLines(before, after);
	for (let i = 0; i < parts.length; i++) {
		const part = parts[i];
		if (!part) continue;
		const next = parts[i + 1];
		if (part.removed && next?.added) {
			const removed = lines(part.value);
			const added = lines(next.value);
			for (let j = 0; j < Math.max(removed.length, added.length); j++) {
				const old = removed[j];
				const now = added[j];
				if (old !== undefined && now !== undefined) {
					const [a, b] = pieces(old, now);
					rows.push({
						kind: "change",
						left: { line: left++, text: old, pieces: a },
						right: { line: right++, text: now, pieces: b },
					});
				} else if (old !== undefined) {
					rows.push({ kind: "remove", left: { line: left++, text: old } });
				} else if (now !== undefined) {
					rows.push({ kind: "add", right: { line: right++, text: now } });
				}
			}
			i++;
		} else if (part.removed) {
			for (const text of lines(part.value))
				rows.push({ kind: "remove", left: { line: left++, text } });
		} else if (part.added) {
			for (const text of lines(part.value))
				rows.push({ kind: "add", right: { line: right++, text } });
		} else {
			for (const text of lines(part.value))
				rows.push({
					kind: "same",
					left: { line: left++, text },
					right: { line: right++, text },
				});
		}
	}
	return rows;
}

/** Unchanged lines kept around each change. */
const CONTEXT = 3;

/**
 * Folds long runs of unchanged lines, keeping {@link CONTEXT} lines next to
 * each change; folds whose `at` is in `open` stay unfolded.
 */
export function fold(rows: Row[], open: ReadonlySet<number> = new Set()): Row[] {
	const shown: Row[] = [];
	let i = 0;
	while (i < rows.length) {
		if (rows[i]?.kind !== "same") {
			shown.push(rows[i] as Row);
			i++;
			continue;
		}
		let end = i;
		while (end < rows.length && rows[end]?.kind === "same") end++;
		const keepBefore = i === 0 ? 0 : CONTEXT;
		const keepAfter = end === rows.length ? 0 : CONTEXT;
		const hidden = end - i - keepBefore - keepAfter;
		if (hidden > 1 && !open.has(i)) {
			shown.push(...rows.slice(i, i + keepBefore));
			shown.push({ kind: "fold", at: i, hidden });
			shown.push(...rows.slice(end - keepAfter, end));
		} else {
			shown.push(...rows.slice(i, end));
		}
		i = end;
	}
	return shown;
}

/** Git's status of a file as one letter, as in VS Code. */
export function statusLetter(status: string): string {
	switch (status) {
		case "added":
			return "A";
		case "deleted":
			return "D";
		case "renamed":
			return "R";
		case "copied":
			return "C";
		case "typeChanged":
			return "T";
		case "untracked":
			return "U";
		case "conflicted":
			return "!";
		default:
			return "M";
	}
}

/** The parts of a file's change the one list needs. */
export interface ChangeLike {
	path: string;
	oldPath?: string | null;
	status: string;
	added?: number | null;
	removed?: number | null;
}

/** Whether a file's changes are staged: all, some, or none. */
export type Staged = "all" | "some" | "none";

/** A file in the one list of changes: staged and unstaged parts together. */
export interface Entry<C extends ChangeLike = ChangeLike> {
	path: string;
	/** What the row shows: the working tree's change, else the staged one. */
	change: C;
	staged: Staged;
	/** Which side its diff shows when opened. */
	group: "changes" | "staged" | "untracked";
}

/**
 * Git's staged, unstaged and untracked groups as one list, each file once,
 * sorted by path: checked when its changes are all staged, mixed when only
 * some are.
 */
export function entries<C extends ChangeLike>(status: {
	staged: C[];
	changes: C[];
	untracked: C[];
}): Entry<C>[] {
	const byPath = new Map<string, Entry<C>>();
	for (const change of status.staged)
		byPath.set(change.path, { path: change.path, change, staged: "all", group: "staged" });
	for (const change of status.changes) {
		const before = byPath.get(change.path);
		byPath.set(change.path, {
			path: change.path,
			change,
			staged: before ? "some" : "none",
			group: "changes",
		});
	}
	for (const change of status.untracked)
		byPath.set(change.path, { path: change.path, change, staged: "none", group: "untracked" });
	return [...byPath.values()].toSorted((a, b) => a.path.localeCompare(b.path));
}

/** How a set of files is staged, taken together. */
export function stagedOf(items: { staged: Staged }[]): Staged {
	if (items.length === 0) return "none";
	if (items.every((item) => item.staged === "all")) return "all";
	if (items.every((item) => item.staged === "none")) return "none";
	return "some";
}

/** A folder of the tree of changes, or a file in it. */
export type TreeNode<E> =
	| { kind: "folder"; path: string; name: string; children: TreeNode<E>[]; files: E[] }
	| { kind: "file"; entry: E };

/**
 * Files as a tree of folders, folders first, each sorted by name. A folder
 * holding only one folder shows as one (`src/app/shell`), as in VS Code.
 */
export function tree<E extends { path: string }>(items: E[]): TreeNode<E>[] {
	interface Draft {
		folders: Map<string, Draft>;
		files: E[];
	}
	const root: Draft = { folders: new Map(), files: [] };
	for (const item of items) {
		const parts = item.path.split("/");
		let at = root;
		for (const part of parts.slice(0, -1)) {
			let next = at.folders.get(part);
			if (!next) {
				next = { folders: new Map(), files: [] };
				at.folders.set(part, next);
			}
			at = next;
		}
		at.files.push(item);
	}
	const all = (draft: Draft): E[] => [...draft.files, ...[...draft.folders.values()].flatMap(all)];
	const build = (draft: Draft, prefix: string): TreeNode<E>[] => {
		const folders = [...draft.folders.entries()]
			.toSorted(([a], [b]) => a.localeCompare(b))
			.map(([name, child]): TreeNode<E> => {
				let label = name;
				let node = child;
				let only = node.files.length === 0 ? [...node.folders.entries()] : [];
				while (only.length === 1 && only[0]) {
					const [next, inner] = only[0];
					label = `${label}/${next}`;
					node = inner;
					only = node.files.length === 0 ? [...node.folders.entries()] : [];
				}
				const path = `${prefix}${label}`;
				return {
					kind: "folder",
					path,
					name: label,
					children: build(node, `${path}/`),
					files: all(node),
				};
			});
		const files = draft.files
			.toSorted((a, b) => a.path.localeCompare(b.path))
			.map((entry): TreeNode<E> => ({ kind: "file", entry }));
		return [...folders, ...files];
	};
	return build(root, "");
}

/** A commit's row of the graph: its lane, and the lines through the row. */
export interface GraphRow {
	/** The commit's lane. */
	lane: number;
	/** Lanes passing straight through the row (the same lane at top and bottom). */
	through: number[];
	/** Lanes coming in from above that end at this commit (its children). */
	into: number[];
	/** Lanes leaving the commit below, towards its parents. */
	out: number[];
	/** Lanes from above that bend into the commit's lane below it (same parent). */
	joins: number[];
	/** Lanes at the bottom of the row, for drawing below it (a commit's files). */
	below: number[];
	/** Whether a lane comes into the commit from above (it has a child shown). */
	child: boolean;
}

/**
 * The lanes of a commit graph, from each commit's parents. Commits come
 * newest first, parents after their children (`--topo-order`); each lane
 * waits for the commit it leads to, and a commit takes the lane waiting for
 * it, or a free one when nothing is.
 */
export function graph(commits: { hash: string; parents: string[] }[]): {
	rows: GraphRow[];
	width: number;
} {
	const lanes: (string | null)[] = [];
	let width = 1;
	const rows = commits.map((commit) => {
		const waiting = lanes.flatMap((hash, index) => (hash === commit.hash ? [index] : []));
		const free = () => {
			const index = lanes.indexOf(null);
			return index === -1 ? lanes.length : index;
		};
		const lane = waiting[0] ?? free();
		const before = lanes.slice();
		for (const index of waiting) lanes[index] = null;
		const out: number[] = [];
		const joins: number[] = [];
		commit.parents.forEach((parent, index) => {
			const already = lanes.indexOf(parent);
			// A line to the right waiting for the same parent joins this one,
			// so the first parent's line stays straight (git's `|/`).
			if (index === 0 && already > lane) {
				lanes[already] = null;
				lanes[lane] = parent;
				joins.push(already);
				out.push(lane);
				return;
			}
			if (already !== -1) {
				out.push(already);
				return;
			}
			const target = index === 0 ? lane : free();
			lanes[target] = parent;
			out.push(target);
		});
		while (lanes.length && lanes.at(-1) === null) lanes.pop();
		const through = before.flatMap((hash, index) =>
			hash !== null && hash !== commit.hash && lanes[index] === hash ? [index] : [],
		);
		width = Math.max(width, before.length, lanes.length, lane + 1);
		const below = lanes.flatMap((hash, index) => (hash === null ? [] : [index]));
		return {
			lane,
			through,
			into: waiting.filter((index) => index !== lane),
			out,
			joins,
			below,
			child: waiting.length > 0,
		};
	});
	return { rows, width };
}

/** A ref on a commit, as the graph shows it. */
export interface Badge {
	name: string;
	kind: "head" | "branch" | "remote" | "tag";
}

/**
 * `%D`'s refs as badges: the current branch first (`HEAD` itself only when
 * detached), local branches, tags, then remote branches. A remote branch
 * at the same commit as its local one is left out (it is in step).
 */
export function badges(refs: string[], remotes: string[] = ["origin"]): Badge[] {
	const head = refs.indexOf("HEAD");
	const current = head !== -1 ? refs[head + 1] : undefined;
	const isRemote = (ref: string) => remotes.some((remote) => ref.startsWith(`${remote}/`));
	const locals = new Set(refs.filter((ref) => !isRemote(ref) && !ref.startsWith("tag: ")));
	const list: Badge[] = [];
	if (head !== -1 && (!current || isRemote(current) || current.startsWith("tag: ")))
		list.push({ name: "HEAD", kind: "head" });
	for (const ref of refs) {
		if (ref === "HEAD" || ref.endsWith("/HEAD")) continue;
		if (ref.startsWith("tag: ")) list.push({ name: ref.slice(5), kind: "tag" });
		else if (isRemote(ref)) {
			const local = ref.slice(ref.indexOf("/") + 1);
			if (!locals.has(local)) list.push({ name: ref, kind: "remote" });
		} else list.push({ name: ref, kind: ref === current ? "head" : "branch" });
	}
	const order = { head: 0, branch: 1, tag: 2, remote: 3 };
	return list.toSorted((a, b) => order[a.kind] - order[b.kind]);
}

/** A run of changed rows: a hunk, with the lines it removes and adds. */
export interface Hunk {
	/** Index of its first row in the rows. */
	at: number;
	/** Old lines it removes and new lines it adds, by number. */
	removed: number[];
	added: number[];
}

/** Lines to take from the new side, by number on each side. */
export interface Taken {
	removed: ReadonlySet<number>;
	added: ReadonlySet<number>;
}

/** The hunks of a diff's rows. */
export function hunks(rows: Row[]): Hunk[] {
	const list: Hunk[] = [];
	let current: Hunk | undefined;
	rows.forEach((row, index) => {
		if (row.kind === "same" || row.kind === "fold") {
			current = undefined;
			return;
		}
		if (!current) {
			current = { at: index, removed: [], added: [] };
			list.push(current);
		}
		if ("left" in row) current.removed.push(row.left.line);
		if ("right" in row) current.added.push(row.right.line);
	});
	return list;
}

/** A text's lines with their line endings, as diffLines counts them. */
function rawLines(text: string): string[] {
	return text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
}

/**
 * `before` with only some of the changes towards `after` made: removed
 * lines in `take.removed` go, added lines in `take.added` come; the rest
 * stays as in `before`. Staging lines makes the index this from the index
 * and the file; unstaging makes it from the last commit and the index,
 * taking every change but those unstaged.
 */
export function applyLines(before: string, after: string, take: Taken): string {
	const out: string[] = [];
	let left = 1;
	let right = 1;
	for (const part of diffLines(before, after)) {
		for (const line of rawLines(part.value)) {
			if (part.added) {
				if (take.added.has(right)) out.push(line);
				right++;
			} else if (part.removed) {
				if (!take.removed.has(left)) out.push(line);
				left++;
			} else {
				out.push(line);
				left++;
				right++;
			}
		}
	}
	return out.join("");
}

/** Every change of the rows, less some: what unstaging those leaves. */
export function allBut(rows: Row[], but: Taken): Taken {
	const removed = new Set<number>();
	const added = new Set<number>();
	for (const row of rows) {
		if (row.kind === "same" || row.kind === "fold") continue;
		if ("left" in row && !but.removed.has(row.left.line)) removed.add(row.left.line);
		if ("right" in row && !but.added.has(row.right.line)) added.add(row.right.line);
	}
	return { removed, added };
}

/** A checkout in a repository group: its path, branch and short name. */
export interface Checkout {
	path: string;
	branch?: string | null;
	/** Its name within the group: its folder in the repository's, else its own. */
	label: string;
}

/** Checkouts of one repository; a group of one is shown as before. */
export interface RepositoryGroup {
	/** The repository's folder (relative to the project's when inside it). */
	repository: string;
	checkouts: Checkout[];
}

/**
 * The checkouts found in a folder, grouped by the repository they check out
 * (a repository and its worktrees), in the order first seen.
 */
export function groupCheckouts(
	found: { path: string; branch?: string | null; repository?: string | null }[],
): RepositoryGroup[] {
	const groups = new Map<string, RepositoryGroup>();
	for (const item of found) {
		const repository = item.repository ?? item.path;
		const label = item.path.startsWith(`${repository}/`)
			? item.path.slice(repository.length + 1)
			: (item.path.split("/").at(-1) ?? item.path);
		const group = groups.get(repository) ?? { repository, checkouts: [] };
		group.checkouts.push({ path: item.path, branch: item.branch, label });
		groups.set(repository, group);
	}
	return [...groups.values()];
}

/**
 * A hunk as unified diff text, for an agent: its removed (`-`)
 * and added (`+`) lines with up to `context` unchanged lines around them,
 * and the new side's lines it covers.
 */
export function hunkText(
	rows: Row[],
	hunk: Hunk,
	context = 3,
): { text: string; startLine: number; endLine: number } {
	let end = hunk.at;
	while (end + 1 < rows.length && rows[end + 1]?.kind !== "same" && rows[end + 1]?.kind !== "fold")
		end++;
	let from = hunk.at;
	for (let n = 0; n < context && from > 0 && rows[from - 1]?.kind === "same"; n++) from--;
	let to = end;
	for (let n = 0; n < context && to + 1 < rows.length && rows[to + 1]?.kind === "same"; n++) to++;
	const out: string[] = [];
	const covered: number[] = [];
	for (let index = from; index <= to; index++) {
		const row = rows[index];
		if (!row || row.kind === "fold") continue;
		if (row.kind === "same") {
			out.push(` ${row.right.text}`);
			covered.push(row.right.line);
			continue;
		}
		if ("left" in row) out.push(`-${row.left.text}`);
		if ("right" in row) {
			out.push(`+${row.right.text}`);
			covered.push(row.right.line);
		}
	}
	const startLine = covered.length ? Math.min(...covered) : 1;
	const endLine = covered.length ? Math.max(...covered) : startLine;
	return { text: out.join("\n"), startLine, endLine };
}
