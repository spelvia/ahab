/**
 * A project's problems: grouped by file, filtered, counted, and
 * an agent's turn's difference in words. Pure: no React, no API.
 */
import type {
	CountsDto,
	ProblemDto,
	SeverityDto,
	TurnDiagnosticsDto,
} from "@/shared/api/generated/ahab.schemas";
import { formatList, t } from "@/shared/i18n";

const RANK: Record<string, number> = { error: 0, warning: 1, information: 2, hint: 3 };

/** A severity's place, most severe first; an unknown one counts as a hint. */
export function rank(severity: string): number {
	return RANK[severity] ?? 3;
}

/** What the list shows. */
export interface ProblemFilters {
	/** The lowest severity shown. */
	severity: SeverityDto;
	/** Only this tool's; empty for every tool. */
	source: string;
	/** In the message, code or path; any case. */
	text: string;
}

export const allProblems: ProblemFilters = { severity: "hint", source: "", text: "" };

/** Whether a problem passes the filters. */
export function matches(problem: ProblemDto, filters: ProblemFilters): boolean {
	if (rank(problem.severity) > rank(filters.severity)) return false;
	if (filters.source && problem.source !== filters.source) return false;
	const text = filters.text.trim().toLowerCase();
	if (!text) return true;
	return [problem.message, problem.code ?? "", problem.path].some((part) =>
		part.toLowerCase().includes(text),
	);
}

/** One file's problems. */
export interface FileProblems {
	path: string;
	problems: ProblemDto[];
	counts: CountsDto;
}

/** Each file once, its worst first, then by name; inside, by place. */
export function groupByFile(problems: readonly ProblemDto[]): FileProblems[] {
	const files = new Map<string, ProblemDto[]>();
	for (const problem of problems) {
		const list = files.get(problem.path);
		if (list) list.push(problem);
		else files.set(problem.path, [problem]);
	}
	const groups = [...files].map(([path, list]) => ({
		path,
		problems: list.toSorted(
			(a, b) => a.line - b.line || a.column - b.column || rank(a.severity) - rank(b.severity),
		),
		counts: countOf(list),
	}));
	return groups.toSorted(
		(a, b) => worst(a.problems) - worst(b.problems) || a.path.localeCompare(b.path),
	);
}

function worst(problems: readonly ProblemDto[]): number {
	return Math.min(...problems.map((problem) => rank(problem.severity)));
}

/** Counts by severity. */
export function countOf(problems: readonly ProblemDto[]): CountsDto {
	const counts = { errors: 0, warnings: 0, information: 0, hints: 0 };
	for (const problem of problems) {
		const severity = rank(problem.severity);
		if (severity === 0) counts.errors += 1;
		else if (severity === 1) counts.warnings += 1;
		else if (severity === 2) counts.information += 1;
		else counts.hints += 1;
	}
	return counts;
}

/** Every problem counted. */
export function total(counts: CountsDto): number {
	return counts.errors + counts.warnings + counts.information + counts.hints;
}

/** The tools that reported, for the source filter. */
export function sourcesOf(problems: readonly ProblemDto[]): string[] {
	return [...new Set(problems.map((problem) => problem.source))].toSorted();
}

/** "3 errors, 1 warning"; "No problems" when there are none. */
export function countText(counts: CountsDto): string {
	const parts = [
		counts.errors ? t("tools:counts.errors", { count: counts.errors }) : "",
		counts.warnings ? t("tools:counts.warnings", { count: counts.warnings }) : "",
		counts.information ? t("tools:counts.information", { count: counts.information }) : "",
		counts.hints ? t("tools:counts.hints", { count: counts.hints }) : "",
	].filter(Boolean);
	return parts.length ? formatList(parts, "unit") : t("tools:counts.none");
}

/**
 * A turn's difference: "3 new errors, 1 fixed". New warnings are named
 * when there are no new errors; hints and information only add to fixed.
 */
export function turnText(added: CountsDto, fixed: CountsDto): string {
	const parts: string[] = [];
	if (added.errors) parts.push(t("tools:turn.newErrors", { count: added.errors }));
	if (added.warnings) parts.push(t("tools:turn.newWarnings", { count: added.warnings }));
	const other = added.information + added.hints;
	if (other) parts.push(t("tools:turn.newNotes", { count: other }));
	const gone = total(fixed);
	if (gone) parts.push(t("tools:turn.fixed", { count: gone }));
	return parts.length ? formatList(parts, "unit") : t("tools:turn.none");
}

/** Where a problem is: `src/main.rs:12:5`. */
function place(problem: ProblemDto): string {
	return `${problem.path}:${problem.line}:${problem.column}`;
}

/** A problem on one line, as tools print it: `src/main.rs:12:5: error[E0308]: … (rustc)`. */
export function problemLine(problem: ProblemDto): string {
	const code = problem.code ? `[${problem.code}]` : "";
	return `${place(problem)}: ${problem.severity}${code}: ${problem.message} (${problem.source})`;
}

/** A turn's new problems, one per line, for the agent. */
export function turnProblemsText(diagnostics: TurnDiagnosticsDto): string {
	return diagnostics.files.flatMap((file) => file.new.map(problemLine)).join("\n");
}

// --- The Problems panel's filter ---------------------------------------------

/**
 * A glob as a regular expression, as VS Code reads the Problems filter's:
 * `**` any folders (none too), `*` and `?` inside a name, `{a,b}` either,
 * `[abc]` a class. A glob without a slash matches a name anywhere
 * (`*.ts`); one with a slash matches from the project's folder.
 */
export function globToRegExp(glob: string): RegExp {
	const pattern = glob.replace(/^\.?\//, "");
	const anywhere = !pattern.includes("/");
	let out = "";
	let braces = 0;
	for (let index = 0; index < pattern.length; index++) {
		const char = pattern[index] ?? "";
		if (char === "*" && pattern[index + 1] === "*") {
			index++;
			if (pattern[index + 1] === "/") {
				index++;
				out += "(?:.*/)?";
			} else out += ".*";
		} else if (char === "*") out += "[^/]*";
		else if (char === "?") out += "[^/]";
		else if (char === "{") {
			braces++;
			out += "(?:";
		} else if (char === "}" && braces > 0) {
			braces--;
			out += ")";
		} else if (char === "," && braces > 0) out += "|";
		else if (char === "[") {
			const close = pattern.indexOf("]", index + 1);
			if (close === -1) out += "\\[";
			else {
				const inside = pattern
					.slice(index + 1, close)
					.replace(/^!/, "^")
					.replace(/\\/g, "\\\\");
				out += `[${inside}]`;
				index = close;
			}
		} else out += char.replace(/[.+^$()|\\\]]/g, "\\$&");
	}
	out += ")".repeat(braces);
	// A folder matches what is inside it: `src/test` takes `src/test/a.ts`.
	return new RegExp(`^${anywhere ? "(?:.*/)?" : ""}${out}(?:/.*)?$`);
}

/** Whether a filter's term is a glob rather than words. */
function isGlob(term: string): boolean {
	return /[*?[{]/.test(term) || term.includes("/");
}

/** The filter box's text, read: words and globs to keep, and to leave out (`!`). */
export interface ParsedFilter {
	words: string[];
	globs: RegExp[];
	notWords: string[];
	notGlobs: RegExp[];
}

/** Splits at commas outside braces: `**\/*.{ts,tsx}, error`. */
function terms(text: string): string[] {
	const found: string[] = [];
	let depth = 0;
	let current = "";
	for (const char of text) {
		if (char === "{") depth++;
		if (char === "}") depth = Math.max(0, depth - 1);
		if (char === "," && depth === 0) {
			found.push(current);
			current = "";
		} else current += char;
	}
	found.push(current);
	return found.map((term) => term.trim()).filter(Boolean);
}

/**
 * Reads the filter box: terms split by commas; a term with a glob
 * character or a slash is a glob over paths, else words found in the
 * message, code, tool or path (any case); `!` before either leaves out
 * what it matches.
 */
export function parseFilter(text: string): ParsedFilter {
	const parsed: ParsedFilter = { words: [], globs: [], notWords: [], notGlobs: [] };
	for (const raw of terms(text)) {
		const not = raw.startsWith("!");
		const term = not ? raw.slice(1).trim() : raw;
		if (!term) continue;
		if (isGlob(term)) (not ? parsed.notGlobs : parsed.globs).push(globToRegExp(term));
		else (not ? parsed.notWords : parsed.words).push(term.toLowerCase());
	}
	return parsed;
}

/** Which severities show: errors, warnings, and information (with hints). */
export interface SeverityToggles {
	errors: boolean;
	warnings: boolean;
	infos: boolean;
}

export const ALL_SEVERITIES: SeverityToggles = { errors: true, warnings: true, infos: true };

/** The panel's filters: the box's text read, the severities shown, one tool or every one. */
export interface PanelFilters {
	filter: ParsedFilter;
	severities: SeverityToggles;
	source: string;
}

/**
 * Whether a problem shows in the panel: of a severity shown and the tool
 * chosen, its path matching a glob when there are some, its text holding
 * one of the words when there are some, and none of what is left out.
 */
export function panelMatches(problem: ProblemDto, filters: PanelFilters): boolean {
	const level = rank(problem.severity);
	const { severities, filter } = filters;
	if (level === 0 ? !severities.errors : level === 1 ? !severities.warnings : !severities.infos)
		return false;
	if (filters.source && problem.source !== filters.source) return false;
	const path = problem.path;
	if (filter.notGlobs.some((glob) => glob.test(path))) return false;
	if (filter.globs.length && !filter.globs.some((glob) => glob.test(path))) return false;
	if (!filter.words.length && !filter.notWords.length) return true;
	const text = [problem.message, problem.code ?? "", problem.source, path].join("\n").toLowerCase();
	if (filter.notWords.some((word) => text.includes(word))) return false;
	return !filter.words.length || filter.words.some((word) => text.includes(word));
}

/** Whether the filters hide anything at all. */
export function filtering(filters: PanelFilters): boolean {
	const { filter, severities } = filters;
	return (
		Boolean(filters.source) ||
		!severities.errors ||
		!severities.warnings ||
		!severities.infos ||
		filter.words.length + filter.globs.length + filter.notWords.length + filter.notGlobs.length > 0
	);
}

/** A repository or worktree in the project, as the Git view lists it. */
export interface Checkout {
	/** Its folder in the project. */
	path: string;
	/** The repository it checks out (the same for all its worktrees). */
	repository?: string | null;
	/** `repository` or `worktree`; absent from older machines. */
	kind?: string | null;
	branch?: string | null;
}

/** A repository's or worktree's use, as the server keeps it. */
export interface Activity {
	repository: string;
	/** The worktree's folder; none for the repository's own (or all of it, when pinned). */
	worktree?: string | null;
	pinned: boolean;
	active: boolean;
}

/** Whether a checkout is pinned, and whether it is active (pinned or used lately). */
export function checkoutState(
	checkout: Checkout,
	activity: readonly Activity[],
): { pinned: boolean; active: boolean } {
	const repository = checkout.repository ?? checkout.path;
	const worktree = checkout.path === repository ? null : checkout.path;
	const own = activity.find(
		(row) => row.repository === repository && (row.worktree ?? null) === worktree,
	);
	const whole = activity.find(
		(row) => row.repository === repository && !row.worktree && row.pinned,
	);
	const pinned = Boolean(own?.pinned || whole);
	return { pinned, active: pinned || Boolean(own?.active) };
}

/** One repository or worktree's files with problems. */
interface CheckoutProblems {
	path: string;
	/** Its name under its repository (`main` for `app/main`). */
	label: string;
	branch?: string | null;
	worktree: boolean;
	pinned: boolean;
	active: boolean;
	files: FileProblems[];
	counts: CountsDto;
}

/** One repository's worktrees (or its own checkout) with problems. */
interface RepositoryProblems {
	repository: string;
	/** Pinned as a whole. */
	pinned: boolean;
	checkouts: CheckoutProblems[];
	/** Its one checkout is its own: files go straight under it. */
	single: boolean;
	counts: CountsDto;
}

/** Problems grouped repository → worktree → file, and what is left out. */
export interface Grouped {
	repositories: RepositoryProblems[];
	/** Files in no repository. */
	loose: FileProblems[];
	/** Repositories and worktrees not shown: not active, not pinned, without problems. */
	notChecked: Checkout[];
}

/**
 * Problems grouped repository → worktree → file
 * (Problems, grouped). Shown: active and pinned repositories and worktrees, and those
 * with problems (they were checked); the rest are counted as not checked.
 * Repositories in the order given; a file goes to the deepest checkout
 * that holds it. `all` are the problems before any filter: whether a
 * checkout has problems at all.
 */
export function groupByRepository(
	problems: readonly ProblemDto[],
	checkouts: readonly Checkout[],
	activity: readonly Activity[],
	all: readonly ProblemDto[] = problems,
): Grouped {
	const deepestFirst = checkouts.toSorted((a, b) => b.path.length - a.path.length);
	const homeOf = (path: string) =>
		deepestFirst.find((each) => path === each.path || path.startsWith(`${each.path}/`));
	const withProblems = new Set(all.map((problem) => homeOf(problem.path)?.path));
	const byCheckout = new Map<string, ProblemDto[]>();
	const loose: ProblemDto[] = [];
	for (const problem of problems) {
		const home = homeOf(problem.path);
		if (!home) {
			loose.push(problem);
			continue;
		}
		const list = byCheckout.get(home.path);
		if (list) list.push(problem);
		else byCheckout.set(home.path, [problem]);
	}
	const repositories = new Map<string, RepositoryProblems>();
	const notChecked: Checkout[] = [];
	for (const checkout of checkouts) {
		const repository = checkout.repository ?? checkout.path;
		const state = checkoutState(checkout, activity);
		const list = byCheckout.get(checkout.path) ?? [];
		if (!state.active && !withProblems.has(checkout.path)) {
			notChecked.push(checkout);
			continue;
		}
		if (!list.length) continue;
		const group = repositories.get(repository) ?? {
			repository,
			pinned: activity.some((row) => row.repository === repository && !row.worktree && row.pinned),
			checkouts: [],
			single: false,
			counts: countOf([]),
		};
		const label = checkout.path.startsWith(`${repository}/`)
			? checkout.path.slice(repository.length + 1)
			: (checkout.path.split("/").at(-1) ?? checkout.path);
		group.checkouts.push({
			path: checkout.path,
			label,
			branch: checkout.branch,
			worktree: checkout.path !== repository || checkout.kind === "worktree",
			...state,
			files: groupByFile(list),
			counts: countOf(list),
		});
		repositories.set(repository, group);
	}
	for (const group of repositories.values()) {
		group.single = group.checkouts.length === 1 && !group.checkouts[0]?.worktree;
		group.counts = countOf(
			group.checkouts.flatMap((checkout) => checkout.files.flatMap((file) => file.problems)),
		);
	}
	return {
		repositories: [...repositories.values()],
		loose: groupByFile(loose),
		notChecked,
	};
}
