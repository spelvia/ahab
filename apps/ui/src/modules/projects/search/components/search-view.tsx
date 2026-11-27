import { cn } from "cn";
import type { TFunction } from "i18next";
import { CaseSensitive, ChevronRight, MoreHorizontal, Regex, WholeWord } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useSearch } from "@/modules/projects/search/api";
import { globs } from "@/modules/projects/search/model";
import type { SearchMatch, Project } from "@/shared/api/generated/ahab.schemas";
import { explain } from "@/shared/api/problems";
import { AddToChat } from "@/shared/chat-context/add-to-chat";
import { referenceId } from "@/shared/chat-context/model";
import type { HighlightToken } from "@/shared/editor/model";
import { useTranslation } from "@/shared/i18n";
import { rememberSearch, savedSearch } from "@/shared/preferences/store";
import { useHighlightedLines } from "@/shared/syntax/code";
import { type Span, spansOf, tokenClasses } from "@/shared/syntax/model";
import { FileIcon } from "@/shared/ui/file-icon";

/** The query, as typed, after a short pause. */
function useSettled<T>(value: T, wait = 300): T {
	const [settled, setSettled] = useState(value);
	useEffect(() => {
		const timer = setTimeout(() => setSettled(value), wait);
		return () => clearTimeout(timer);
	}, [value, wait]);
	return settled;
}

/**
 * Searching the project's files' contents, as VS Code's Search view:
 * match case, whole word and regular expression; files to include and
 * exclude; results by file, each line opening the file there.
 */
export function SearchView({
	project,
	folder,
	onOpen,
	autoFocus,
}: {
	project: Project;
	/** Search only in this folder (from the file tree's "Find in folder"). */
	folder?: string;
	/** `beside`: open it to the side of the current window (⌘/Ctrl-click). */
	onOpen: (path: string, line: number, beside: boolean) => void;
	autoFocus?: boolean;
}) {
	const { t } = useTranslation("projects");
	// The last search in this project comes back.
	const [saved] = useState(() => savedSearch(project.id));
	const [text, setText] = useState(saved?.query ?? "");
	const [matchCase, setMatchCase] = useState(saved?.matchCase ?? false);
	const [wholeWord, setWholeWord] = useState(saved?.wholeWord ?? false);
	const [regex, setRegex] = useState(saved?.regex ?? false);
	const [include, setInclude] = useState(folder ? `${folder}/**` : (saved?.include ?? ""));
	const [exclude, setExclude] = useState(saved?.exclude ?? "");
	const [ignored, setIgnored] = useState(saved?.includeIgnored ?? false);
	const [more, setMore] = useState(Boolean(include || exclude || ignored));
	const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
	// One object per change, so the pause below only restarts on typing.
	const wanted = useMemo(
		() => ({
			query: text,
			matchCase,
			wholeWord,
			regex,
			include: globs(include),
			exclude: globs(exclude),
			includeIgnored: ignored,
		}),
		[text, matchCase, wholeWord, regex, include, exclude, ignored],
	);
	const query = useSettled(wanted);
	const search = useSearch(project.id, query);
	const data = search.data;
	useEffect(() => {
		rememberSearch(project.id, {
			query: query.query,
			matchCase: query.matchCase,
			wholeWord: query.wholeWord,
			regex: query.regex,
			include: query.include.join(", "),
			exclude: query.exclude.join(", "),
			includeIgnored: query.includeIgnored,
		});
	}, [project.id, query]);

	return (
		<div className="flex flex-col gap-1.5 px-2 pb-4">
			<div className="border-input bg-card focus-within:ring-ring/40 flex items-center rounded-md border pr-1 focus-within:ring-2">
				<input
					aria-label={t("searchView.query")}
					// oxlint-disable-next-line jsx-a11y/no-autofocus
					autoFocus={autoFocus}
					value={text}
					onChange={(event) => setText(event.target.value)}
					placeholder={t("searchView.query")}
					className="min-w-0 flex-1 bg-transparent px-2 py-1.5 text-[0.8125rem] outline-none"
				/>
				<Toggle
					label={t("searchView.matchCase")}
					on={matchCase}
					onClick={() => setMatchCase((v) => !v)}
				>
					<CaseSensitive />
				</Toggle>
				<Toggle
					label={t("searchView.wholeWord")}
					on={wholeWord}
					onClick={() => setWholeWord((v) => !v)}
				>
					<WholeWord />
				</Toggle>
				<Toggle label={t("searchView.regex")} on={regex} onClick={() => setRegex((v) => !v)}>
					<Regex />
				</Toggle>
			</div>
			<button
				type="button"
				aria-expanded={more}
				onClick={() => setMore((v) => !v)}
				className="text-muted-foreground hover:text-foreground flex items-center gap-1 self-end text-[0.6875rem]"
			>
				<MoreHorizontal className="size-3.5" />
				{more ? t("searchView.fewerOptions") : t("searchView.moreOptions")}
			</button>
			{more ? (
				<div className="flex flex-col gap-1.5">
					<Field
						label={t("searchView.include")}
						value={include}
						onChange={setInclude}
						placeholder={t("searchView.includePlaceholder")}
					/>
					<Field
						label={t("searchView.exclude")}
						value={exclude}
						onChange={setExclude}
						placeholder={t("searchView.excludePlaceholder")}
					/>
					<label className="text-muted-foreground flex items-center gap-1.5 text-xs">
						<input
							type="checkbox"
							checked={ignored}
							onChange={(event) => setIgnored(event.target.checked)}
						/>
						{t("searchView.includeIgnored")}
					</label>
				</div>
			) : null}

			{search.isError ? (
				<p role="alert" className="text-destructive px-1 text-xs">
					{explain(search.error, t("searchView.failed"))}
				</p>
			) : null}
			{data && text && (data.done || data.files.length > 0) ? (
				<div className="flex items-start gap-2">
					<p className="text-muted-foreground min-w-0 flex-1 px-1 text-xs" aria-live="polite">
						{summary(data, t)}
					</p>
					{data.total > 0 ? (
						<AddToChat
							compact
							className="shrink-0"
							references={() => [
								{
									id: referenceId(),
									kind: "searchResults",
									label: t("searchView.reference", { query: text }),
									text: data.files
										.flatMap((file) =>
											file.matches.map(
												(match) => `${file.path}:${match.line}: ${match.preview.trim()}`,
											),
										)
										.join("\n"),
								},
							]}
						/>
					) : null}
				</div>
			) : search.isFetching ? (
				<p className="text-muted-foreground px-1 text-xs">{t("searchView.searching")}</p>
			) : null}

			<ul aria-label={t("searchView.results")}>
				{(text ? (data?.files ?? []) : []).map((file) => {
					const open = !closed.has(file.path);
					const slash = file.path.lastIndexOf("/");
					return (
						<li key={file.path}>
							<button
								type="button"
								aria-expanded={open}
								onClick={() =>
									setClosed((set) => {
										const next = new Set(set);
										if (next.has(file.path)) next.delete(file.path);
										else next.add(file.path);
										return next;
									})
								}
								className="hover:bg-accent/60 flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-[0.8125rem]"
								title={file.path}
							>
								<ChevronRight
									className={cn(
										"text-muted-foreground size-3.5 shrink-0 transition-transform",
										open && "rotate-90",
									)}
								/>
								<FileIcon path={file.path} className="size-3.5" />
								<span className="truncate font-medium">{file.path.slice(slash + 1)}</span>
								<span className="text-muted-foreground min-w-0 flex-1 truncate text-[0.6875rem]">
									{file.path.slice(0, Math.max(0, slash))}
								</span>
								<span className="bg-muted shrink-0 rounded-full px-1.5 text-[0.625rem]">
									{file.matches.length}
									{file.more ? "+" : ""}
								</span>
							</button>
							{open ? <Matches path={file.path} matches={file.matches} onOpen={onOpen} /> : null}
						</li>
					);
				})}
			</ul>
		</div>
	);
}

/** What was found, in a line: how many, and whether the search stopped short. */
function summary(
	data: { total: number; files: unknown[]; done: boolean; cut?: string | null },
	t: TFunction<"projects">,
): string {
	let line =
		data.total === 0
			? t("searchView.summary.none")
			: t("searchView.summary.results", { count: data.total, files: data.files.length });
	if (!data.done) line = t("searchView.summary.searching", { summary: line });
	if (data.cut === "matches") line = t("searchView.summary.cutAtLimit", { summary: line });
	if (data.cut === "time") line = t("searchView.summary.cutAfterTime", { summary: line });
	return line;
}

/** A file's matching lines, highlighted as its language all at once. */
function Matches({
	path,
	matches,
	onOpen,
}: {
	path: string;
	matches: SearchMatch[];
	onOpen: (path: string, line: number, aside: boolean) => void;
}) {
	const { t } = useTranslation("projects");
	const lines = useHighlightedLines(
		useMemo(() => matches.map((match) => match.preview.trimEnd()).join("\n"), [matches]),
		{ path },
	);
	return (
		<ul>
			{matches.map((match, index) => (
				<li key={match.line}>
					<button
						type="button"
						onClick={(event) => onOpen(path, match.line, event.metaKey || event.ctrlKey)}
						title={t("searchView.openMatch")}
						className="hover:bg-accent/60 flex w-full gap-2 rounded py-0.5 pr-1 pl-6 text-left font-mono text-xs"
					>
						<span className="text-muted-foreground w-8 shrink-0 text-right">{match.line}</span>
						<Preview match={match} tokens={lines?.[index]} />
					</button>
				</li>
			))}
		</ul>
	);
}

/** Spans with where each starts in the line: their keys. */
function withOffsets(spans: Span[]): { span: Span; offset: number }[] {
	let offset = 0;
	return spans.map((span) => {
		const at = offset;
		offset += span.text.length;
		return { span, offset: at };
	});
}

/** Characters kept before the first match, so it stays in sight. */
const LEAD = 24;

/**
 * A matching line with its matches marked, starting near the first match,
 * and highlighted when `tokens` are there. The matches' ranges count
 * characters (code points); tokens count UTF-16 units.
 */
function Preview({ match, tokens }: { match: SearchMatch; tokens?: HighlightToken[] }) {
	const chars = [...match.preview.trimEnd()];
	const ranges = match.ranges as [number, number][];
	const first = ranges[0]?.[0] ?? 0;
	// Leading spaces go; a long start is cut so the match shows.
	let at = chars.findIndex((char) => char !== " " && char !== "\t");
	if (at < 0 || at > first) at = 0;
	const cut = first - at > LEAD;
	if (cut) at = first - LEAD;
	const unit = (index: number) => chars.slice(0, index).join("").length;
	const start = unit(at);
	const text = chars.slice(at).join("");
	const shift = (list: { from: number; to: number }[]) =>
		list
			.map((item) => ({ ...item, from: item.from - start, to: item.to - start }))
			.filter((item) => item.to > 0);
	const marks = shift(ranges.map(([from, to]) => ({ from: unit(from), to: unit(to) })));
	return (
		<span className="min-w-0 truncate whitespace-pre">
			{cut ? "…" : null}
			{withOffsets(spansOf(text, tokens ? (shift(tokens) as HighlightToken[]) : [], marks)).map(
				({ span, offset }) => {
					const classes = span.tag ? tokenClasses(span.tag) : undefined;
					return span.marked ? (
						<mark
							key={offset}
							className={cn("bg-warning/35 text-foreground rounded-[2px]", classes)}
						>
							{span.text}
						</mark>
					) : (
						<span key={offset} className={classes}>
							{span.text}
						</span>
					);
				},
			)}
		</span>
	);
}

function Toggle({
	label,
	on,
	onClick,
	children,
}: {
	label: string;
	on: boolean;
	onClick: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			aria-pressed={on}
			title={label}
			onClick={onClick}
			className={cn(
				"grid size-6 place-items-center rounded outline-none [&_svg]:size-4",
				on
					? "bg-primary/20 text-foreground ring-primary/50 ring-1"
					: "text-muted-foreground hover:text-foreground",
			)}
		>
			{children}
		</button>
	);
}

function Field({
	label,
	value,
	onChange,
	placeholder,
}: {
	label: string;
	value: string;
	onChange: (value: string) => void;
	placeholder: string;
}) {
	return (
		<label className="text-muted-foreground text-[0.6875rem]">
			{label}
			<input
				value={value}
				onChange={(event) => onChange(event.target.value)}
				placeholder={placeholder}
				className="border-input bg-card text-foreground focus-visible:ring-ring/40 mt-0.5 block w-full rounded-md border px-2 py-1 font-mono text-xs outline-none focus-visible:ring-2"
			/>
		</label>
	);
}
