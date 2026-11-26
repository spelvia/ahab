import {
	Archive,
	Check,
	Folder as FolderIcon,
	FolderMinus,
	ListFilter,
	Pin,
	PinOff,
	Trash2,
	X,
} from "lucide-react";
import { useState } from "react";

import {
	useArchiveSession,
	useDeleteSession,
	useUpdateSession,
} from "@/modules/sessions/session/api";
import { isBusy, type SessionGrouping, type SessionSort } from "@/modules/sessions/session/model";
import type { Session, SessionFolder } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { toast, toastError } from "@/shared/ui/toast";

const SORTS: SessionSort[] = ["activity", "created", "name"];
const GROUPINGS: SessionGrouping[] = ["folders", "none", "status", "agent", "day"];

/** How the Sessions view orders and groups its sessions (kept on this device). */
export function ListOptions({
	sort,
	group,
	onChange,
}: {
	sort: SessionSort;
	group: SessionGrouping;
	onChange: (view: { sort: SessionSort; group: SessionGrouping }) => void;
}) {
	const { t } = useTranslation("sessions");
	const changed = sort !== "activity" || group !== "folders";
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				aria-label={t("sessionList.view.label")}
				title={t("sessionList.view.label")}
				className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring relative grid size-8 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2"
			>
				<ListFilter className="size-4" />
				{changed ? (
					<span
						className="bg-primary absolute top-1.5 right-1.5 size-1.5 rounded-full"
						aria-hidden
					/>
				) : null}
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="w-52">
				<p className="text-muted-foreground px-2 py-1 text-xs font-medium">
					{t("sessionList.view.sortBy")}
				</p>
				{SORTS.map((option) => (
					<DropdownMenuItem key={option} onSelect={() => onChange({ sort: option, group })}>
						<Check className={option === sort ? "" : "invisible"} />
						{t(`sessionList.view.sort.${option}`)}
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<p className="text-muted-foreground px-2 py-1 text-xs font-medium">
					{t("sessionList.view.groupBy")}
				</p>
				{GROUPINGS.map((option) => (
					<DropdownMenuItem key={option} onSelect={() => onChange({ sort, group: option })}>
						<Check className={option === group ? "" : "invisible"} />
						{t(`sessionList.view.group.${option}`)}
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

/**
 * Above the list while sessions are selected: what can be done to them all
 * at once. Busy sessions are not archived (each asks first, from its menu);
 * only archived ones are deleted.
 */
export function SelectionBar({
	sessions,
	folders,
	onClear,
	onGone,
}: {
	sessions: Session[];
	folders: SessionFolder[];
	onClear: () => void;
	onGone?: (session: Session) => void;
}) {
	const { t } = useTranslation("sessions");
	const update = useUpdateSession();
	const archive = useArchiveSession();
	const remove = useDeleteSession();
	const [deleting, setDeleting] = useState(false);
	const all = async (run: (session: Session) => Promise<unknown>, list = sessions) => {
		const results = await Promise.allSettled(list.map(run));
		const failed = results.find((result) => result.status === "rejected");
		if (failed?.status === "rejected")
			toastError(failed.reason instanceof Error ? failed.reason : new Error(String(failed.reason)));
	};
	const change = (data: { folderId?: string | null; pinned?: boolean }) =>
		void all((session) => update.mutateAsync({ id: session.id, data })).then(onClear);
	const archivable = sessions.filter((session) => !session.archivedAt && !isBusy(session));
	const busy = sessions.filter((session) => !session.archivedAt && isBusy(session)).length;
	const allArchived = sessions.every((session) => session.archivedAt);
	const anyPinned = sessions.some((session) => session.pinnedAt);
	const button =
		"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-7 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2 disabled:opacity-40";
	return (
		<div
			role="toolbar"
			aria-label={t("sessionList.selection.label", { count: sessions.length })}
			className="border-border bg-card mx-2 mb-2 flex items-center gap-0.5 rounded-md border py-0.5 pr-0.5 pl-2 text-xs"
		>
			<span className="text-heading min-w-0 flex-1 truncate font-medium">
				{t("sessionList.selection.count", { count: sessions.length })}
			</span>
			{allArchived ? null : (
				<>
					<DropdownMenu>
						<DropdownMenuTrigger
							className={button}
							aria-label={t("sessionList.selection.move")}
							title={t("sessionList.selection.move")}
						>
							<FolderIcon className="size-3.5" />
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="max-h-72 w-52 overflow-auto">
							{folders.map((folder) => (
								<DropdownMenuItem key={folder.id} onSelect={() => change({ folderId: folder.id })}>
									<FolderIcon />
									<span className="truncate">{folder.name}</span>
								</DropdownMenuItem>
							))}
							{folders.length ? <DropdownMenuSeparator /> : null}
							<DropdownMenuItem onSelect={() => change({ folderId: null })}>
								<FolderMinus />
								{t("sessionList.selection.noFolder")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
					<button
						type="button"
						className={button}
						onClick={() => change({ pinned: !anyPinned })}
						aria-label={
							anyPinned ? t("sessionList.selection.unpin") : t("sessionList.selection.pin")
						}
						title={anyPinned ? t("sessionList.selection.unpin") : t("sessionList.selection.pin")}
					>
						{anyPinned ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
					</button>
					<button
						type="button"
						className={button}
						disabled={archivable.length === 0}
						onClick={() =>
							void all((session) => archive.mutateAsync({ id: session.id }), archivable).then(
								() => {
									for (const session of archivable) onGone?.(session);
									if (busy) toast(t("sessionList.selection.busyKept", { count: busy }));
									onClear();
								},
							)
						}
						aria-label={t("sessionList.selection.archive")}
						title={
							busy
								? t("sessionList.selection.archiveBusy", { count: busy })
								: t("sessionList.selection.archive")
						}
					>
						<Archive className="size-3.5" />
					</button>
				</>
			)}
			{allArchived ? (
				<button
					type="button"
					className={button}
					onClick={() => setDeleting(true)}
					aria-label={t("sessionList.selection.delete")}
					title={t("sessionList.selection.delete")}
				>
					<Trash2 className="size-3.5" />
				</button>
			) : null}
			<button
				type="button"
				className={button}
				onClick={onClear}
				aria-label={t("sessionList.selection.clear")}
				title={t("sessionList.selection.clear")}
			>
				<X className="size-3.5" />
			</button>
			<ConfirmDialog
				open={deleting}
				onOpenChange={setDeleting}
				title={t("sessionList.selection.deleteTitle", { count: sessions.length })}
				description={t("sessionList.selection.deleteDescription")}
				action={t("sessionList.selection.delete")}
				destructive
				pending={remove.isPending}
				onConfirm={() =>
					void all((session) => remove.mutateAsync({ id: session.id })).then(() => {
						for (const session of sessions) onGone?.(session);
						setDeleting(false);
						onClear();
					})
				}
			/>
		</div>
	);
}
