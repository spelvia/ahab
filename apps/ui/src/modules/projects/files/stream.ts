/**
 * Following open files' changes on disk: one
 * stream per project for all its open file windows, opened again (a
 * moment later) when the set of files changes. The machine first says each
 * file's version, then each change; a window compares them with what it
 * shows.
 */

/** A file's version on disk now: null when it is gone. */
export type FileListener = (version: string | null) => void;

interface Watch {
	listeners: Map<string, Set<FileListener>>;
	source?: EventSource;
	timer?: ReturnType<typeof setTimeout>;
	failures: number;
	/** The paths the open stream follows. */
	following: string;
}

const watches = new Map<string, Watch>();
/** Before the stream opens again after the set of files changed. */
const SETTLE = 150;
/** Before a broken stream is tried again. */
const RETRY = 2_000;
const MAX_RETRY = 30_000;

function tell(watch: Watch, path: string, version: string | null) {
	for (const listener of watch.listeners.get(path) ?? []) listener(version);
}

function connect(projectId: string, watch: Watch) {
	watch.timer = undefined;
	const paths = [...watch.listeners.keys()].toSorted();
	const following = paths.join("\n");
	if (watch.source && following === watch.following) return;
	watch.source?.close();
	watch.source = undefined;
	watch.following = following;
	if (paths.length === 0) {
		watches.delete(projectId);
		return;
	}
	if (typeof EventSource === "undefined") return;
	const query = paths.map((path) => `path=${encodeURIComponent(path)}`).join("&");
	const source = new EventSource(`/api/v1/projects/${projectId}/files/watch?${query}`);
	watch.source = source;
	const retry = () => {
		if (watch.source !== source) return;
		source.close();
		watch.source = undefined;
		clearTimeout(watch.timer);
		const delay = Math.min(MAX_RETRY, RETRY * 2 ** watch.failures++);
		watch.timer = setTimeout(() => connect(projectId, watch), delay);
	};
	source.addEventListener("message", (message) => {
		if (watch.source !== source) return;
		const frame = JSON.parse(String(message.data)) as {
			type: string;
			versions?: Record<string, string | null>;
			path?: string;
			version?: string | null;
		};
		if (frame.type === "watching") {
			watch.failures = 0;
			for (const [path, version] of Object.entries(frame.versions ?? {}))
				tell(watch, path, version);
		} else if (frame.type === "changed" && frame.path) {
			tell(watch, frame.path, frame.version ?? null);
		} else if (frame.type === "unwatched") {
			retry();
		}
	});
	source.addEventListener("error", retry);
}

function schedule(projectId: string, watch: Watch) {
	clearTimeout(watch.timer);
	watch.timer = setTimeout(() => connect(projectId, watch), SETTLE);
}

/** Follows a file's version on disk; returns a way to stop. */
export function watchFile(projectId: string, path: string, listener: FileListener): () => void {
	let watch = watches.get(projectId);
	if (!watch) {
		watch = { listeners: new Map(), following: "", failures: 0 };
		watches.set(projectId, watch);
	}
	const own = watch;
	const set = own.listeners.get(path) ?? new Set();
	set.add(listener);
	own.listeners.set(path, set);
	schedule(projectId, own);
	return () => {
		set.delete(listener);
		if (set.size === 0) own.listeners.delete(path);
		schedule(projectId, own);
	};
}
