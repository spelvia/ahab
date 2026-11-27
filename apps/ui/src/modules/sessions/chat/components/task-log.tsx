import { ArrowDown } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { readTaskOutput } from "@/modules/sessions/chat/api";
import { Status, taskWhat } from "@/modules/sessions/chat/components/activity-strip";
import { shellSummary } from "@/modules/sessions/chat/components/command-text";
import { TerminalLog } from "@/modules/sessions/chat/components/terminal-output";
import { FullCommand } from "@/modules/sessions/chat/components/tool-detail";
import { active, type ObservedTask } from "@/modules/sessions/chat/model";
import { problemCode } from "@/shared/api/fetch";
import { useTranslation } from "@/shared/i18n";
import { CopyButton } from "@/shared/ui/copy-button";
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/shared/ui/dialog";

/** The most output kept on screen; older text is dropped from the top. */
const MAX_TEXT = 2_000_000;
/** How often a running task's log is read again once at its end. */
const POLL_MS = 1_000;
/** How much is read on opening, and on each "Read more", before waiting for the reader. */
const READ_AHEAD = 512 * 1024;
/** How long typing in the filter waits before the log is filtered again. */
const FILTER_MS = 150;

/** The lines of `text` containing `query`, case-insensitively; null when none do. */
export function filterLines(text: string, query: string): string | null {
	if (!query) return text;
	const wanted = query.toLowerCase();
	const lines = text.split("\n").filter((line) => line.toLowerCase().includes(wanted));
	return lines.length ? lines.join("\n") : null;
}

type Read =
	| { state: "loading" }
	| { state: "shown"; text: string; partial: boolean; unavailable: boolean }
	| { state: "unsupported" }
	| { state: "failed"; message: string };

/**
 * A background task's output in a dialog, read page by page and followed as
 * it grows while the task runs (Stubb's task output). It stays at the end
 * unless scrolled up to read.
 */
export function TaskLog({
	chatId,
	task,
	onClose,
}: {
	chatId: string;
	/** The task shown, as last observed; none closes the dialog. */
	task: ObservedTask | undefined;
	onClose: () => void;
}) {
	const { t } = useTranslation("sessions");
	return (
		<Dialog open={task !== undefined} onOpenChange={(open) => !open && onClose()}>
			{task ? (
				<DialogContent size="xl">
					<DialogHeader>
						<DialogTitle className="flex min-w-0 items-center gap-3 text-sm">
							<span className="min-w-0 flex-1 truncate font-mono">
								{(task.kind.type === "command"
									? shellSummary(task.kind.command)
									: taskWhat(task)) || t("taskLog.title")}
							</span>
							<span className="text-xs font-normal">
								<Status status={task.status} />
							</span>
						</DialogTitle>
						{task.description && task.kind.type === "command" ? (
							<p className="text-muted-foreground text-xs">{task.description}</p>
						) : null}
						{task.kind.type === "command" ? (
							<details className="text-xs">
								<summary className="text-muted-foreground cursor-pointer">
									{t("taskLog.showCommand")}
								</summary>
								<div className="mt-1.5">
									<FullCommand command={task.kind.command} />
								</div>
							</details>
						) : null}
					</DialogHeader>
					<DialogBody className="flex min-h-0 flex-1 flex-col p-0">
						<Output key={`${chatId}:${task.id}`} chatId={chatId} task={task} />
					</DialogBody>
				</DialogContent>
			) : null}
		</Dialog>
	);
}

function Output({ chatId, task }: { chatId: string; task: ObservedTask }) {
	const { t } = useTranslation("sessions");
	const [wrap, setWrap] = useState(true);
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState("");
	useEffect(() => {
		const timer = setTimeout(() => setFilter(query), FILTER_MS);
		return () => clearTimeout(timer);
	}, [query]);
	const [read, setRead] = useState<Read>({ state: "loading" });
	// Set while reading waits for the reader to ask for more: how to go on.
	const [more, setMore] = useState<((all: boolean) => void) | undefined>();
	// Read in the loop below, which outlives a render.
	const running = useRef(active(task.status));
	useEffect(() => {
		running.current = active(task.status);
	}, [task.status]);

	useEffect(() => {
		// Where reading got to, and what it read.
		const at: {
			cursor?: string;
			text: string;
			timer?: ReturnType<typeof setTimeout>;
			stopped: boolean;
			/** Read so far, and how far to read before waiting. */
			read: number;
			budget: number;
		} = { text: "", stopped: false, read: 0, budget: READ_AHEAD };
		// Each page is read from where the last one ended, never again from
		// an earlier place: no `nextCursor` means the log is complete.
		const next = async () => {
			try {
				const page = await readTaskOutput(chatId, task.id, at.cursor);
				if (at.stopped) return;
				at.text = (at.text + page.text).slice(-MAX_TEXT);
				at.read += page.text.length;
				setRead({
					state: "shown",
					text: at.text,
					partial: page.availability === "partial",
					unavailable: page.availability === "unavailable",
				});
				if (!page.nextCursor) return;
				at.cursor = page.nextCursor;
				// More is written already: read it now, or once asked to.
				if (!page.eof && page.text) {
					if (at.read < at.budget) at.timer = setTimeout(next, 0);
					else
						setMore(() => (all: boolean) => {
							if (at.stopped) return;
							at.budget = all ? Infinity : at.read + READ_AHEAD;
							setMore(undefined);
							void next();
						});
				}
				// At the end: wait while it runs, and read once more after it finished.
				else if (running.current || page.text) at.timer = setTimeout(next, POLL_MS);
			} catch (error) {
				if (at.stopped) return;
				setRead((shown) =>
					// What was read stays shown.
					shown.state === "shown"
						? shown
						: problemCode(error) === "unsupported"
							? { state: "unsupported" }
							: {
									state: "failed",
									message: error instanceof Error ? error.message : String(error),
								},
				);
			}
		};
		void next();
		return () => {
			at.stopped = true;
			clearTimeout(at.timer);
		};
	}, [chatId, task.id]);

	// Stays at the end as output arrives, unless scrolled up.
	const scroller = useRef<HTMLPreElement>(null);
	const following = useRef(true);
	const lastTop = useRef(0);
	const [away, setAway] = useState(false);
	// After each render, which is when new text may have grown past the end.
	useLayoutEffect(() => {
		const view = scroller.current;
		if (view && following.current) view.scrollTop = view.scrollHeight;
	});

	const shownText = useMemo(
		() => (read.state === "shown" ? filterLines(read.text, filter) : null),
		[read, filter],
	);

	if (read.state === "loading")
		return <p className="text-muted-foreground p-4 text-sm">{t("taskLog.loading")}</p>;
	if (read.state === "unsupported")
		return <p className="text-muted-foreground p-4 text-sm">{t("taskLog.unsupported")}</p>;
	if (read.state === "failed")
		return (
			<p role="alert" className="text-destructive p-4 text-sm">
				{t("taskLog.failed", { message: read.message })}
			</p>
		);
	return (
		<div className="relative flex min-h-0 flex-1 flex-col">
			<div className="flex items-center gap-2 border-b px-3 py-1 text-xs">
				<input
					type="search"
					className="min-w-0 flex-1 bg-transparent p-1"
					aria-label={t("tools.filterLog")}
					placeholder={t("tools.filterLog")}
					value={query}
					onChange={(event) => setQuery(event.target.value)}
				/>
				<button type="button" aria-pressed={wrap} onClick={() => setWrap(!wrap)}>
					{t("tools.wrap")}
				</button>
				<CopyButton text={read.text} label={t("tools.output")} />
			</div>
			{read.partial ? (
				<p className="text-muted-foreground border-border border-b px-4 py-1.5 text-xs">
					{t("taskLog.partial")}
				</p>
			) : null}
			<pre
				ref={scroller}
				onScroll={(event) => {
					const view = event.currentTarget;
					const below = view.scrollHeight - view.scrollTop - view.clientHeight;
					if (below < 40) following.current = true;
					else if (view.scrollTop < lastTop.current) following.current = false;
					lastTop.current = view.scrollTop;
					setAway(below > 200);
				}}
				tabIndex={0}
				className={
					"bg-muted/40 h-[60vh] min-h-0 overflow-auto px-4 py-3 font-mono text-xs leading-5 " +
					(wrap ? "break-all whitespace-pre-wrap" : "whitespace-pre")
				}
			>
				{read.text ? (
					<TerminalLog text={shownText ?? t("tools.noMatch")} />
				) : read.unavailable ? (
					<span className="text-muted-foreground font-sans">{t("taskLog.unavailable")}</span>
				) : (
					<span className="text-muted-foreground font-sans">{t("taskLog.empty")}</span>
				)}
			</pre>
			{more ? (
				<div className="text-muted-foreground flex items-center gap-3 border-t px-4 py-1.5 text-xs">
					<span className="flex-1">{t("taskLog.more")}</span>
					<button type="button" className="hover:underline" onClick={() => more(false)}>
						{t("taskLog.readMore")}
					</button>
					<button type="button" className="hover:underline" onClick={() => more(true)}>
						{t("taskLog.readAll")}
					</button>
				</div>
			) : null}
			{away ? (
				<button
					type="button"
					aria-label={t("taskLog.toEnd")}
					title={t("taskLog.toEnd")}
					onClick={() => {
						following.current = true;
						scroller.current?.scrollTo({ top: scroller.current.scrollHeight });
					}}
					className={
						(more ? "bottom-14 " : "bottom-4 ") +
						"bg-popover border-border hover:bg-accent absolute right-4 grid size-8 place-items-center rounded-full border shadow-lg"
					}
				>
					<ArrowDown className="size-4" />
				</button>
			) : null}
		</div>
	);
}
