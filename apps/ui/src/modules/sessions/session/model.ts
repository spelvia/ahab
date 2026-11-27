import type { Chat, Session, SessionFolder, Terminal } from "@/shared/api/generated/ahab.schemas";
import { i18n } from "@/shared/i18n";

/** Whether archiving would stop an agent or a terminal at work, and so should be asked first. */
export function isBusy(session: Session): boolean {
	if (session.terminals.some((terminal) => terminal.state === "running")) return true;
	return session.state === "working" || session.state === "needsYou";
}

/** A chat's or terminal's tab in its session: `chat:<id>`, `terminal:<id>`. */
type PartKey = `chat:${string}` | `terminal:${string}`;

/** What a session's tab shows: one of its chats or terminals. */
export type Part =
	| { key: PartKey; kind: "chat"; chat: Chat; createdAt: string }
	| { key: PartKey; kind: "terminal"; terminal: Terminal; createdAt: string };

/** A session's chats and terminals, as its tabs show them: oldest first. */
export function partsOf(session: Pick<Session, "chats" | "terminals">): Part[] {
	const parts: Part[] = [
		...session.chats.map((chat): Part => ({
			key: `chat:${chat.id}`,
			kind: "chat",
			chat,
			createdAt: chat.createdAt,
		})),
		...session.terminals.map((terminal): Part => ({
			key: `terminal:${terminal.id}`,
			kind: "terminal",
			terminal,
			createdAt: terminal.createdAt,
		})),
	];
	return parts.toSorted(
		(a, b) => a.createdAt.localeCompare(b.createdAt) || a.key.localeCompare(b.key),
	);
}

/** A part's name: a chat's title; a terminal's name, its shell's title, or its command. */
export function partTitle(part: Part): string {
	if (part.kind === "chat") return part.chat.title;
	const { terminal } = part;
	return (
		terminal.label ?? terminal.title ?? terminal.command ?? i18n.t("sessions:sessionView.terminal")
	);
}

/** Whether it still runs: a live chat, a running terminal. */
export function partLive(part: Part): boolean {
	return part.kind === "chat" ? part.chat.state === "live" : part.terminal.state === "running";
}

/** A part's state in a dot and a word, as the session list shows a session's. */
export function partState(part: Part): { dot: string; label: string; waiting: boolean } {
	if (part.kind === "terminal") {
		const { terminal } = part;
		if (terminal.state === "running")
			return {
				dot: "border border-success",
				label: i18n.t("sessions:state.running"),
				waiting: false,
			};
		return {
			dot: "border border-muted-foreground/60",
			label:
				terminal.exitCode !== null && terminal.exitCode !== undefined
					? i18n.t("sessions:state.exitedWith", { code: terminal.exitCode })
					: i18n.t("sessions:state.exited"),
			waiting: false,
		};
	}
	const { chat } = part;
	if (chat.state !== "live")
		return {
			dot: "border border-muted-foreground/60",
			label: i18n.t("sessions:state.ended"),
			waiting: false,
		};
	if (chat.openRequests.length > 0)
		return { dot: "bg-warning", label: i18n.t("sessions:state.needsYou"), waiting: true };
	if (chat.status === "working" || chat.status === "starting")
		return {
			dot: "bg-success animate-pulse",
			label: i18n.t("sessions:state.working"),
			waiting: false,
		};
	return { dot: "border border-success", label: i18n.t("sessions:state.idle"), waiting: false };
}

/** A folder in the Sessions view's tree, with what it holds. */
export interface FolderNode {
	folder: SessionFolder;
	folders: FolderNode[];
	sessions: Session[];
	/** Sessions that need you, in it or its folders. */
	waiting: number;
	/** Sessions in it or its folders. */
	count: number;
}

/** Pinned first (in the order pinned), then `rest`. */
function pinnedFirst<T extends { pinnedAt?: string | null }>(
	items: T[],
	rest: (a: T, b: T) => number,
): T[] {
	return items.toSorted((a, b) => {
		if (a.pinnedAt && b.pinnedAt) return a.pinnedAt.localeCompare(b.pinnedAt);
		if (a.pinnedAt) return -1;
		if (b.pinnedAt) return 1;
		return rest(a, b);
	});
}

function byParent<T>(items: T[], parentOf: (item: T) => string | null) {
	const groups = new Map<string | null, T[]>();
	for (const item of items) {
		const parent = parentOf(item);
		const siblings = groups.get(parent);
		if (siblings) siblings.push(item);
		else groups.set(parent, [item]);
	}
	return groups;
}

/**
 * The Sessions view's tree: folders in folders, each holding its sessions;
 * at each level pinned ones first, then folders by name and sessions by
 * latest activity. A session in a folder that is gone, or a folder whose
 * parent is gone (or that loops), sits at the top level.
 */
export function sessionTree(
	sessions: Session[],
	folders: SessionFolder[],
	sort: SessionSort = "activity",
): { folders: FolderNode[]; sessions: Session[] } {
	const known = new Map(folders.map((folder) => [folder.id, folder]));
	// A folder's parent counts only when following parents ends at the top.
	const parentOf = (folder: SessionFolder): string | null => {
		const seen = new Set([folder.id]);
		let at = folder.parentId ?? null;
		while (at) {
			if (seen.has(at) || !known.has(at)) return null;
			seen.add(at);
			at = known.get(at)?.parentId ?? null;
		}
		return folder.parentId && known.has(folder.parentId) ? folder.parentId : null;
	};
	const children = byParent(folders, parentOf);
	const filed = byParent(sessions, (session) =>
		session.folderId && known.has(session.folderId) ? session.folderId : null,
	);
	const ordered = (parent: string | null) =>
		pinnedFirst(filed.get(parent) ?? [], sessionOrder(sort));
	const build = (parent: string | null): FolderNode[] =>
		pinnedFirst(children.get(parent) ?? [], (a, b) => a.name.localeCompare(b.name)).map(
			(folder) => {
				const inner = build(folder.id);
				const own = ordered(folder.id);
				return {
					folder,
					folders: inner,
					sessions: own,
					waiting:
						own.filter((session) => session.state === "needsYou").length +
						inner.reduce((sum, node) => sum + node.waiting, 0),
					count: own.length + inner.reduce((sum, node) => sum + node.count, 0),
				};
			},
		);
	return { folders: build(null), sessions: ordered(null) };
}

/** Whether folder `id` is `ancestor` or inside it (so it cannot go there). */
export function folderWithin(folders: SessionFolder[], id: string, ancestor: string): boolean {
	const known = new Map(folders.map((folder) => [folder.id, folder]));
	let at: string | null | undefined = id;
	for (let step = 0; at && step <= folders.length; step += 1) {
		if (at === ancestor) return true;
		at = known.get(at)?.parentId;
	}
	return false;
}

/** How sessions are ordered in the Sessions view. */
export type SessionSort = "activity" | "created" | "name";

/** How the Sessions view groups sessions: by their folders, or by something they have. */
export type SessionGrouping = "folders" | "none" | "status" | "agent" | "day";

/** Latest activity first, newest first, or by title. */
function sessionOrder(sort: SessionSort): (a: Session, b: Session) => number {
	switch (sort) {
		case "created":
			return (a, b) => b.createdAt.localeCompare(a.createdAt);
		case "name":
			return (a, b) =>
				a.title.localeCompare(b.title, undefined, { sensitivity: "base", numeric: true });
		default:
			return (a, b) => b.updatedAt.localeCompare(a.updatedAt);
	}
}

/** A heading of the Sessions view when it groups by something other than folders. */
export interface SessionGroup {
	key: string;
	label: string;
	sessions: Session[];
}

/** The four states, in the order they matter: needing you first. */
const STATE_ORDER = ["needsYou", "working", "idle", "ended"];

/**
 * Sessions under headings (their state, their agent, the day of their latest
 * activity), each heading's in `sort` order and the headings in a fixed one.
 * `label` names a heading's key.
 */
export function sessionGroups(
	sessions: Session[],
	grouping: Exclude<SessionGrouping, "folders">,
	sort: SessionSort,
	label: (grouping: "status" | "agent" | "day", key: string) => string,
	now = new Date(),
): SessionGroup[] {
	const ordered = sessions.toSorted(sessionOrder(sort));
	if (grouping === "none") return [{ key: "all", label: "", sessions: ordered }];
	const keyOf = (session: Session): string => {
		if (grouping === "status") return session.state;
		if (grouping === "agent")
			return session.kind === "terminal" ? "terminal" : (session.chats.at(-1)?.agent ?? "none");
		return dayOf(session.updatedAt, now);
	};
	const groups = new Map<string, Session[]>();
	for (const session of ordered) {
		const key = keyOf(session);
		groups.set(key, [...(groups.get(key) ?? []), session]);
	}
	const rank = (key: string) => {
		if (grouping === "status") {
			const at = STATE_ORDER.indexOf(key);
			return at === -1 ? STATE_ORDER.length : at;
		}
		if (grouping === "day") return DAYS.indexOf(key as (typeof DAYS)[number]);
		return 0;
	};
	return [...groups.entries()]
		.map(([key, items]) => ({ key, label: label(grouping, key), sessions: items }))
		.toSorted((a, b) => rank(a.key) - rank(b.key) || a.label.localeCompare(b.label));
}

const DAYS = ["today", "yesterday", "week", "month", "older"] as const;

/** Which of today, yesterday, this week, this month or before `at` is. */
function dayOf(at: string, now: Date): (typeof DAYS)[number] {
	const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
	const time = Date.parse(at);
	const day = 86_400_000;
	if (time >= start) return "today";
	if (time >= start - day) return "yesterday";
	if (time >= start - 6 * day) return "week";
	if (time >= start - 30 * day) return "month";
	return "older";
}

/** The sessions from `anchor` to `to`, as they are listed (a Shift-click). */
export function selectRange(listed: readonly string[], anchor: string, to: string): string[] {
	const from = listed.indexOf(anchor);
	const end = listed.indexOf(to);
	if (from === -1 || end === -1) return [to];
	return listed.slice(Math.min(from, end), Math.max(from, end) + 1);
}
