import { cn } from "cn";
import { Bot, Search, Type, User, X } from "lucide-react";
import { Fragment, useEffect, useState } from "react";

import { revealMessage, useSessionSearch } from "@/modules/sessions/chat";
import { showPart } from "@/modules/sessions/session/hooks";
import type { Session, SessionSearchHit } from "@/shared/api/generated/ahab.schemas";
import { ago } from "@/shared/format";
import { useTranslation } from "@/shared/i18n";

/** `value`, once it stopped changing for `ms`. */
function useSettled<T>(value: T, ms: number): T {
	const [settled, setSettled] = useState(value);
	useEffect(() => {
		const timer = setTimeout(() => setSettled(value), ms);
		return () => clearTimeout(timer);
	}, [value, ms]);
	return settled;
}

/** The words of `query`, as a pattern that finds any of them, any case. */
function wordsPattern(query: string): RegExp | null {
	const words = query
		.split(/\s+/)
		.filter(Boolean)
		.map((word) => word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
	return words.length ? new RegExp(words.join("|"), "gi") : null;
}

/** `text` with the words searched for marked. */
function Marked({ text, query }: { text: string; query: string }) {
	const pattern = wordsPattern(query);
	if (!pattern) return text;
	// Pieces keyed by where they start in the text.
	const pieces: { at: number; text: string; marked: boolean }[] = [];
	let from = 0;
	for (const match of text.matchAll(pattern)) {
		if (match.index > from)
			pieces.push({ at: from, text: text.slice(from, match.index), marked: false });
		pieces.push({ at: match.index, text: match[0], marked: true });
		from = match.index + match[0].length;
	}
	if (from < text.length) pieces.push({ at: from, text: text.slice(from), marked: false });
	return pieces.map((piece) =>
		piece.marked ? (
			<mark key={piece.at} className="bg-primary/25 text-heading rounded-sm px-px">
				{piece.text}
			</mark>
		) : (
			<Fragment key={piece.at}>{piece.text}</Fragment>
		),
	);
}

/**
 * The field at the top of the session list. While it holds words, the
 * list shows what they find in the project's sessions instead: titles, and
 * the messages you and the agents wrote, newest first. Choosing a message
 * opens its session there.
 */
export function SessionSearchField({
	query,
	onQuery,
}: {
	query: string;
	onQuery: (query: string) => void;
}) {
	const { t } = useTranslation("sessions");
	return (
		<div className="px-2 pb-2">
			<div className="border-input bg-background focus-within:ring-ring flex h-8 items-center gap-1.5 rounded-md border px-2 focus-within:ring-2">
				<Search className="text-muted-foreground size-3.5 shrink-0" />
				<input
					type="search"
					value={query}
					onChange={(event) => onQuery(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Escape" && query) {
							event.stopPropagation();
							onQuery("");
						}
					}}
					aria-label={t("sessionSearch.label")}
					placeholder={t("sessionSearch.placeholder")}
					className="placeholder:text-muted-foreground min-w-0 flex-1 bg-transparent text-sm outline-none [&::-webkit-search-cancel-button]:hidden"
				/>
				{query ? (
					<button
						type="button"
						onClick={() => onQuery("")}
						aria-label={t("sessionSearch.clear")}
						className="text-muted-foreground hover:text-foreground rounded p-0.5"
					>
						<X className="size-3.5" />
					</button>
				) : null}
			</div>
		</div>
	);
}

export function SessionSearchResults({
	project,
	query,
	sessions,
	selected,
	onOpen,
}: {
	project: string;
	query: string;
	/** The project's sessions, to open one a result names. */
	sessions: Session[];
	selected?: string;
	onOpen: (session: Session, keep?: boolean) => void;
}) {
	const { t } = useTranslation("sessions");
	const settled = useSettled(query, 250);
	const search = useSessionSearch(project, settled);
	const items = search.data?.items ?? [];

	if (search.isError)
		return (
			<p role="alert" className="text-destructive px-3 text-sm">
				{t("sessionSearch.failed")}
			</p>
		);
	if (!search.data)
		return <p className="text-muted-foreground px-3 text-sm">{t("sessionSearch.searching")}</p>;
	if (items.length === 0)
		return (
			<p className="text-muted-foreground px-3 text-sm" role="status">
				{t("sessionSearch.none", { query: settled.trim() })}
			</p>
		);

	const open = (hit: SessionSearchHit) => {
		const session = sessions.find((item) => item.id === hit.sessionId);
		if (!session) return;
		// The session opens at the chat it is in, at the message.
		if (hit.chatId) showPart(session.id, `chat:${hit.chatId}`);
		if (hit.chatId && hit.seq != null) revealMessage(hit.chatId, hit.seq);
		onOpen(session, true);
	};
	return (
		<ul
			aria-label={t("sessionSearch.results", { count: items.length })}
			className={cn("flex flex-col gap-0.5 px-1", search.isFetching && "opacity-70")}
		>
			{items.map((hit) => {
				const Icon = hit.match === "title" ? Type : hit.match === "userMessage" ? User : Bot;
				const who =
					hit.match === "title"
						? t("sessionSearch.inTitle")
						: hit.match === "userMessage"
							? t("sessionSearch.you")
							: t("sessionSearch.agent");
				return (
					<li key={`${hit.sessionId}:${hit.chatId ?? ""}:${hit.seq ?? "title"}`}>
						<button
							type="button"
							onClick={() => open(hit)}
							className={cn(
								"hover:bg-accent/60 focus-visible:ring-ring w-full rounded-md px-2 py-1.5 text-left outline-none focus-visible:ring-2",
								selected === hit.sessionId && "bg-accent/40",
							)}
						>
							<span className="flex items-center gap-1.5 text-xs">
								<span className="text-heading min-w-0 flex-1 truncate font-medium">
									{hit.sessionTitle}
								</span>
								<span className="text-muted-foreground shrink-0">{ago(hit.at)}</span>
							</span>
							{hit.match === "title" ? null : (
								<span className="text-muted-foreground mt-0.5 line-clamp-3 text-xs leading-snug break-words">
									<Marked text={hit.snippet} query={settled} />
								</span>
							)}
							<span className="text-muted-foreground mt-0.5 flex items-center gap-1 text-[0.6875rem]">
								<Icon className="size-3" aria-hidden />
								{who}
							</span>
						</button>
					</li>
				);
			})}
		</ul>
	);
}
