import { cn } from "cn";
import {
	ArchiveRestore,
	Activity,
	ArrowLeft,
	Check,
	MessageSquarePlus,
	MoreHorizontal,
	SquareTerminal,
	X,
} from "lucide-react";
import { useState } from "react";

import { AgentIcon, MachineStatusDot } from "@/modules/machines";
import { NotificationBell } from "@/modules/notifications";
import type { OpenTab } from "@/modules/projects";
import {
	SessionMenu,
	SessionPartPicker,
	sessionState,
	TitleField,
	useUnarchiveSession,
} from "@/modules/sessions";
import type { Session, Project } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { FileIcon } from "@/shared/ui/file-icon";
import { Sheet, SheetClose, SheetContent } from "@/shared/ui/sheet";

const iconButton =
	"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid place-items-center rounded-md outline-none focus-visible:ring-2";

/** The session's title, renamed in place by clicking it. */
function Title({ session, className }: { session: Session; className?: string }) {
	const { t } = useTranslation("shell");
	const [renaming, setRenaming] = useState(false);
	if (renaming) return <TitleField session={session} onDone={() => setRenaming(false)} />;
	return (
		<button
			type="button"
			onClick={() => !session.archivedAt && setRenaming(true)}
			title={session.archivedAt ? session.title : t("sessionHeader.rename")}
			className={cn("text-heading min-w-0 truncate text-left font-semibold", className)}
		>
			{session.title}
		</button>
	);
}

/** Archived sessions say so, with a way back. */
function Unarchive({ session }: { session: Session }) {
	const { t } = useTranslation("shell");
	const unarchive = useUnarchiveSession();
	return (
		<Button
			size="xs"
			variant="outline"
			disabled={unarchive.isPending}
			onClick={() => unarchive.mutate({ id: session.id })}
		>
			<ArchiveRestore />
			{t("sessionHeader.unarchive")}
		</Button>
	);
}

/**
 * A session's header on a phone, in place of the app's: back to the list,
 * its title with the project and machine (and the machine's connection as
 * a dot), its tabs (in a sheet, with the project's status), the bell and
 * its actions.
 */
export function PhoneSessionHeader({
	session,
	project,
	terminals,
	tabs,
	onBack,
	onLeave,
	onFocusTab,
	onCloseTab,
	onNewSession,
	onStatus,
}: {
	session: Session;
	project?: Project;
	terminals: boolean;
	tabs: OpenTab[];
	onBack: () => void;
	onLeave: () => void;
	onFocusTab: (id: string) => void;
	onCloseTab: (id: string) => void;
	onNewSession: () => void;
	/** Opens the project's status sheet (the status bar's items). */
	onStatus?: () => void;
}) {
	const { t } = useTranslation("shell");
	const [renaming, setRenaming] = useState(false);
	const [sheet, setSheet] = useState(false);
	const state = sessionState(session);
	const archived = Boolean(session.archivedAt);
	return (
		<header className="bg-muted/40 border-border box-content flex h-14 shrink-0 items-center gap-0.5 border-b px-1 pt-[env(safe-area-inset-top)]">
			<button
				type="button"
				onClick={onBack}
				aria-label={t("backToSessions")}
				className={cn(iconButton, "text-foreground size-11")}
			>
				<ArrowLeft className="size-5" />
			</button>
			<div className="min-w-0 flex-1 px-1">
				{renaming ? (
					<TitleField session={session} onDone={() => setRenaming(false)} className="w-full" />
				) : (
					<p className="flex items-center gap-2">
						<span className={cn("size-2 shrink-0 rounded-full", state.dot)} aria-hidden />
						<Title session={session} className="text-[0.9375rem]" />
					</p>
				)}
				<p className="text-muted-foreground flex items-center gap-1.5 text-xs">
					{project ? <MachineStatusDot machineId={project.machineId} className="size-2" /> : null}
					<span className="min-w-0 truncate">
						{project
							? t(archived ? "sessionHeader.archivedWhere" : "sessionHeader.where", {
									project: project.name,
									machine: project.machineName,
								})
							: archived
								? t("sessionHeader.archived")
								: ""}
					</span>
				</p>
			</div>
			<SessionPartPicker
				session={session}
				terminals={terminals}
				phone
				onActivate={() => onFocusTab(`session:${session.id}`)}
			/>
			<button
				type="button"
				onClick={() => setSheet(true)}
				aria-label={t("sessionHeader.tabs", { count: tabs.length })}
				className={cn(iconButton, "size-11")}
			>
				<span className="grid size-6 place-items-center rounded-md border-[1.5px] border-current text-xs font-semibold">
					{tabs.length}
				</span>
			</button>
			<NotificationBell phone />
			<SessionMenu
				session={session}
				onRename={() => setRenaming(true)}
				onArchived={onLeave}
				onDeleted={onLeave}
				onOpen={() => {}}
				trigger={
					<button
						type="button"
						aria-label={t("sessionHeader.actions")}
						className={cn(iconButton, "size-11")}
					>
						<MoreHorizontal className="size-5" />
					</button>
				}
			/>
			<Sheet open={sheet} onOpenChange={setSheet}>
				<SheetContent title={t("sessionHeader.windows")}>
					{archived ? (
						<div className="flex items-center gap-3 px-5 pb-2 text-sm">
							<span className="text-muted-foreground flex-1">{t("sessionHeader.isArchived")}</span>
							<Unarchive session={session} />
						</div>
					) : null}
					<ul className="min-h-0 flex-1 overflow-auto px-2">
						{tabs.map((tab) => {
							// This session's window shows its agent.
							const chat = tab.params.sessionId === session.id ? session.chats[0] : undefined;
							return (
								<li
									key={tab.id}
									className={cn("flex items-center rounded-lg", tab.active && "bg-accent")}
								>
									<SheetClose asChild>
										<button
											type="button"
											onClick={() => onFocusTab(tab.id)}
											className="flex min-h-12 min-w-0 flex-1 items-center gap-3 px-3 text-left"
										>
											{chat ? (
												<AgentIcon agent={chat.agent} className="size-5" />
											) : tab.component === "file" ? (
												<FileIcon path={tab.title} className="size-5" />
											) : (
												<MessageSquarePlus className="text-muted-foreground size-5" />
											)}
											<span className="min-w-0 flex-1 truncate text-[0.9375rem]">{tab.title}</span>
											{tab.active ? <Check className="text-muted-foreground size-4" /> : null}
										</button>
									</SheetClose>
									<button
										type="button"
										onClick={() => onCloseTab(tab.id)}
										aria-label={t("sessionHeader.closeTab", { title: tab.title })}
										className={cn(iconButton, "size-11")}
									>
										<X className="size-4" />
									</button>
								</li>
							);
						})}
						{tabs.length === 0 ? (
							<li className="text-muted-foreground px-3 py-3 text-sm">
								{t("sessionHeader.noTabs")}
							</li>
						) : null}
					</ul>
					<div className="border-border mt-2 grid grid-cols-2 gap-3 border-t px-4 pt-4">
						{onStatus ? (
							<SheetClose asChild>
								<Button
									variant="ghost"
									size="lg"
									className="col-span-2 h-11 justify-start rounded-full"
									onClick={onStatus}
								>
									<Activity />
									{t("statusBar.status")}
								</Button>
							</SheetClose>
						) : null}
						<SheetClose asChild>
							<Button variant="secondary" size="lg" className="rounded-full" onClick={onNewSession}>
								<MessageSquarePlus />
								{t("sessionHeader.newSession")}
							</Button>
						</SheetClose>
						<Button
							variant="secondary"
							size="lg"
							className="rounded-full"
							disabled
							title={t("sessionHeader.terminalLater")}
						>
							<SquareTerminal />
							{t("sessionHeader.newTerminal")}
						</Button>
					</div>
				</SheetContent>
			</Sheet>
		</header>
	);
}
