/** A path matched by a quick open query, with where the letters matched. */
export interface Found {
	path: string;
	score: number;
	/** Character positions in `path` that matched. */
	positions: number[];
}

const BOUNDARY = /[/_\-. ]/;

/**
 * Fuzzy-matches `query` against `path`: its letters in order, anywhere.
 * Matches in the file name, at the start of words and in runs score
 * higher; shorter paths win ties.
 */
export function fuzzy(query: string, path: string): Found | undefined {
	const q = query.toLowerCase().replace(/\s+/g, "");
	if (!q) return { path, score: 0, positions: [] };
	const lower = path.toLowerCase();
	const nameStart = path.lastIndexOf("/") + 1;
	const positions: number[] = [];
	let score = 0;
	let from = 0;
	let previous = -2;
	for (const letter of q) {
		const at = lower.indexOf(letter, from);
		if (at === -1) return undefined;
		positions.push(at);
		let points = 1;
		if (at === previous + 1) points += 5;
		const before = path[at - 1];
		const current = path[at] ?? "";
		if (at === 0 || (before && BOUNDARY.test(before))) points += 8;
		else if (current !== current.toLowerCase() && before === before?.toLowerCase()) points += 6;
		if (at >= nameStart) points += 4;
		score += points;
		previous = at;
		from = at + 1;
	}
	return { path, score: score - path.length * 0.05, positions };
}

/** The best matches for `query`, best first. */
export function rank(query: string, paths: string[], limit = 50): Found[] {
	const found: Found[] = [];
	for (const path of paths) {
		const match = fuzzy(query, path);
		if (match) found.push(match);
	}
	return found.toSorted((a, b) => b.score - a.score).slice(0, limit);
}

/** A glob list typed in a field: `src/**, *.rs` → `["src/**", "*.rs"]`. */
export function globs(text: string): string[] {
	return text
		.split(",")
		.map((glob) => glob.trim())
		.filter(Boolean);
}
