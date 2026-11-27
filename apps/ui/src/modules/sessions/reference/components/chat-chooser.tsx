import { cn } from "cn";
import { Bot, Plus } from "lucide-react";
import { useMemo, useState } from "react";

import { sessionState, useSessions } from "@/modules/sessions/session";
import type { Session } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/shared/ui/dialog";

/**
 * Which chat gets the context: the project's open session
 * windows first, then its recent sessions, then a new session with it.
 * Choosing one makes it the target from then on.
 */
export function ChatChooser({
	open,
	onOpenChange,
	projectId,
	openSessions,
	what,
	onPick,
	onNew,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	projectId: string;
	/** Sessions with a window open here, in their order. */
	openSessions: string[];
	/** What is being added, for the title: "src/join.rs:40–58". */
	what: string;
	onPick: (session: Session) => void;
	onNew: () => void;
}) {
	const { t } = useTranslation("sessions");
	const sessions = useSessions(projectId);
	const [filter, setFilter] = useState("");
	const { opened, recent } = useMemo(() => {
		const chats = (sessions.data?.items ?? []).filter(
			(session) =>
				session.kind === "chat" && !session.archivedAt && session.projectId === projectId,
		);
		const wanted = filter.trim().toLowerCase();
		const shown = wanted
			? chats.filter((session) => session.title.toLowerCase().includes(wanted))
			: chats;
		return {
			opened: openSessions
				.map((id) => shown.find((session) => session.id === id))
				.filter((session): session is Session => Boolean(session)),
			recent: shown
				.filter((session) => !openSessions.includes(session.id))
				.toSorted((a, b) => b.updatedAt.localeCompare(a.updatedAt))
				.slice(0, 12),
		};
	}, [sessions.data, projectId, openSessions, filter]);
	const all = [...opened, ...recent];
	const [active, setActive] = useState(0);
	const choose = (index: number) => {
		const session = all[index];
		if (session) onPick(session);
		else onNew();
		onOpenChange(false);
	};
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="md">
				<div className="border-border border-b px-5 pt-5 pr-14 pb-3.5">
					<DialogTitle>{t("chatChooser.title")}</DialogTitle>
					<DialogDescription className="truncate">{what}</DialogDescription>
					<input
						// oxlint-disable-next-line jsx-a11y/no-autofocus
						autoFocus
						value={filter}
						onChange={(event) => {
							setFilter(event.target.value);
							setActive(0);
						}}
						onKeyDown={(event) => {
							if (event.key === "ArrowDown") {
								event.preventDefault();
								setActive((at) => Math.min(at + 1, all.length));
							} else if (event.key === "ArrowUp") {
								event.preventDefault();
								setActive((at) => Math.max(at - 1, 0));
							} else if (event.key === "Enter") {
								event.preventDefault();
								choose(active);
							}
						}}
						placeholder={t("chatChooser.findPlaceholder")}
						aria-label={t("chatChooser.find")}
						className="border-input bg-background focus-visible:ring-ring mt-3 w-full rounded-md border px-2.5 py-1.5 text-sm outline-none focus-visible:ring-2"
					/>
				</div>
				<ul
					role="listbox"
					aria-label={t("chatChooser.chats")}
					className="max-h-80 overflow-auto p-1.5"
				>
					{opened.length ? <Heading>{t("chatChooser.open")}</Heading> : null}
					{opened.map((session, index) => (
						<Row
							key={session.id}
							session={session}
							active={active === index}
							onChoose={() => choose(index)}
						/>
					))}
					{recent.length ? <Heading>{t("chatChooser.recent")}</Heading> : null}
					{recent.map((session, index) => (
						<Row
							key={session.id}
							session={session}
							active={active === opened.length + index}
							onChoose={() => choose(opened.length + index)}
						/>
					))}
					<li role="option" aria-selected={active === all.length}>
						<button
							type="button"
							onClick={() => choose(all.length)}
							className={cn(
								"text-primary flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm font-medium",
								active === all.length ? "bg-accent" : "hover:bg-accent/60",
							)}
						>
							<Plus className="size-4" />
							{t("chatChooser.newSession")}
						</button>
					</li>
				</ul>
			</DialogContent>
		</Dialog>
	);
}

function Heading({ children }: { children: React.ReactNode }) {
	return (
		<li className="text-muted-foreground px-2.5 pt-2 pb-1 text-[0.6875rem] font-semibold tracking-wide uppercase">
			{children}
		</li>
	);
}

function Row({
	session,
	active,
	onChoose,
}: {
	session: Session;
	active: boolean;
	onChoose: () => void;
}) {
	const state = sessionState(session);
	return (
		<li role="option" aria-selected={active}>
			<button
				type="button"
				onClick={onChoose}
				className={cn(
					"flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm",
					active ? "bg-accent" : "hover:bg-accent/60",
				)}
			>
				<Bot className="text-muted-foreground size-4 shrink-0" />
				<span className="min-w-0 flex-1 truncate">{session.title}</span>
				<span className="text-muted-foreground flex shrink-0 items-center gap-1.5 text-xs">
					<span className={cn("size-2 rounded-full", state.dot)} />
					{state.label}
				</span>
			</button>
		</li>
	);
}
