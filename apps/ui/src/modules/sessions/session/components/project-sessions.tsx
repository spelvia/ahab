import { Link } from "@tanstack/react-router";
import { cn } from "cn";

import { useSessions } from "@/modules/sessions/session/api";
import { sessionState } from "@/modules/sessions/session/components/session-list";
import { shownTitle } from "@/modules/sessions/terminal";
import type { Project } from "@/shared/api/generated/ahab.schemas";
import { ago } from "@/shared/format";
import { useTranslation } from "@/shared/i18n";

/** Sessions listed on a project's card, latest first. */
const SHOWN = 3;

/**
 * What runs in a project, for its card on its Overview: how many
 * sessions need you or work, and the latest ones, each opening in it.
 */
export function ProjectSessions({ project }: { project: Project }) {
	const { t } = useTranslation("sessions");
	const sessions = useSessions(project.id);
	const here = (sessions.data?.items ?? [])
		.filter((session) => !session.archivedAt)
		.toSorted((a, b) => b.updatedAt.localeCompare(a.updatedAt));
	const waiting = here.filter((session) => session.state === "needsYou").length;
	const working = here.filter((session) => session.state === "working").length;
	return (
		<div className="border-border space-y-1.5 border-t pt-3">
			<p className="text-muted-foreground flex flex-wrap gap-x-3 text-xs">
				<span>{t("projectSessions.count", { count: here.length })}</span>
				{waiting ? (
					<span className="text-warning font-medium">
						{t("projectSessions.waiting", { count: waiting })}
					</span>
				) : null}
				{working ? (
					<span className="text-success">{t("projectSessions.working", { count: working })}</span>
				) : null}
			</p>
			{here.length ? (
				<ul>
					{here.slice(0, SHOWN).map((session) => {
						const state = sessionState(session);
						return (
							<li key={session.id}>
								<Link
									to="/p/$project/s/$session"
									params={{ project: project.id, session: session.id }}
									className="hover:bg-accent/60 focus-visible:ring-ring -mx-1.5 flex items-center gap-2 rounded px-1.5 py-1 text-xs outline-none focus-visible:ring-2"
								>
									<span className={cn("size-1.5 shrink-0 rounded-full", state.dot)} aria-hidden />
									<span className="text-heading min-w-0 flex-1 truncate">
										{shownTitle(session)}
									</span>
									<span className="text-muted-foreground shrink-0">{state.label}</span>
									<span className="text-muted-foreground w-8 shrink-0 text-right tabular-nums">
										{ago(session.updatedAt)}
									</span>
								</Link>
							</li>
						);
					})}
				</ul>
			) : null}
		</div>
	);
}
