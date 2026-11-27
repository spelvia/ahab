import { cn } from "cn";
import { GripVertical, ListEnd, Pencil, X, Zap } from "lucide-react";
import { useState } from "react";

import type { QueuedMessage } from "@/modules/sessions/chat/model";
import { useTranslation } from "@/shared/i18n";
import { toastError } from "@/shared/ui/toast";

const iconButton =
	"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2";

/** Moves `id` to where `target` is (or one step, with a number) in `ids`. */
export function moved(ids: readonly string[], id: string, target: string | -1 | 1): string[] {
	const from = ids.indexOf(id);
	const to = typeof target === "number" ? from + target : ids.indexOf(target);
	if (from < 0 || to < 0 || to >= ids.length || to === from) return [...ids];
	const next = ids.filter((each) => each !== id);
	next.splice(to, 0, id);
	return next;
}

/**
 * The messages waiting for the agent's turn to end, each on its own row:
 * read in full by expanding it, changed in place, put in another order by
 * dragging its handle (or Alt+↑ and Alt+↓), sent now or taken back.
 */
export function QueueList({
	queue,
	working,
	onUnqueue,
	onSendNow,
	onReorder,
	onEdit,
}: {
	queue: readonly QueuedMessage[];
	working?: boolean;
	onUnqueue?: (id: string) => void;
	onSendNow?: (id: string) => void;
	/** The waiting messages' IDs in their new order, first first. */
	onReorder?: (order: string[]) => void;
	onEdit?: (id: string, text: string) => Promise<unknown>;
}) {
	const { t } = useTranslation("sessions");
	// Shown at once while the server confirms; the next queue it sends wins.
	const [local, setLocal] = useState<{ of: readonly QueuedMessage[]; ids: string[] }>();
	const order = local?.of === queue ? local.ids : undefined;
	const [dragging, setDragging] = useState<string>();
	const [over, setOver] = useState<string>();
	const [open, setOpen] = useState<string>();
	const [editing, setEditing] = useState<{ id: string; text: string; saving?: boolean }>();
	const byId = new Map(queue.map((item) => [item.id, item]));
	const shown = (order ?? queue.map((item) => item.id))
		.map((id) => byId.get(id))
		.filter((item): item is QueuedMessage => item !== undefined);
	// Interrupting and unconfirmed messages keep their place.
	const movable = shown
		.filter((item) => !item.interrupt && !item.unconfirmed)
		.map((item) => item.id);
	const move = (id: string, target: string | -1 | 1) => {
		if (!onReorder || !movable.includes(id)) return;
		if (typeof target === "string" && !movable.includes(target)) return;
		const next = moved(movable, id, target);
		if (next.join() === movable.join()) return;
		const fixed = shown.filter((item) => !movable.includes(item.id)).map((item) => item.id);
		setLocal({ of: queue, ids: [...fixed, ...next] });
		onReorder(next);
	};
	const save = async () => {
		if (!editing || !onEdit) return;
		setEditing({ ...editing, saving: true });
		try {
			await onEdit(editing.id, editing.text);
			setEditing(undefined);
		} catch (error) {
			toastError(error instanceof Error ? error : new Error(String(error)));
			setEditing((now) => (now ? { ...now, saving: false } : now));
		}
	};
	return (
		<section
			aria-label={t("composer.queued", { count: queue.length })}
			className="border-border bg-card/60 rounded-lg border text-xs"
		>
			<p className="text-muted-foreground flex items-center gap-1.5 px-3 pt-2 pb-1">
				<ListEnd className="size-3.5" aria-hidden />
				{t("composer.queued", { count: queue.length })}
			</p>
			<ul className="divide-border max-h-64 divide-y overflow-auto">
				{shown.map((item) => {
					const canMove = Boolean(onReorder) && movable.includes(item.id) && movable.length > 1;
					const expanded = open === item.id;
					const edit = editing?.id === item.id ? editing : undefined;
					return (
						<li
							key={item.id}
							draggable={canMove && !edit}
							onDragStart={(event) => {
								event.dataTransfer.effectAllowed = "move";
								event.dataTransfer.setData("text/plain", item.id);
								setDragging(item.id);
							}}
							onDragEnd={() => {
								setDragging(undefined);
								setOver(undefined);
							}}
							onDragOver={(event) => {
								if (!dragging || dragging === item.id || !movable.includes(item.id)) return;
								event.preventDefault();
								setOver(item.id);
							}}
							onDragLeave={() => setOver((now) => (now === item.id ? undefined : now))}
							onDrop={(event) => {
								event.preventDefault();
								if (dragging) move(dragging, item.id);
								setDragging(undefined);
								setOver(undefined);
							}}
							onKeyDown={(event) => {
								if (edit || !event.altKey) return;
								if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
								event.preventDefault();
								move(item.id, event.key === "ArrowUp" ? -1 : 1);
							}}
							className={cn(
								"group flex items-start gap-2 px-2 py-2.5",
								dragging === item.id && "opacity-50",
								over === item.id && "ring-primary/60 ring-1 ring-inset",
							)}
						>
							{canMove ? (
								<GripVertical
									aria-hidden
									className="text-muted-foreground/50 group-hover:text-muted-foreground mt-0.5 size-3.5 shrink-0 cursor-grab"
								/>
							) : item.interrupt && !item.unconfirmed ? (
								<Zap
									className="text-primary mt-0.5 size-3.5 shrink-0"
									aria-label={t("composer.queuedNext")}
								/>
							) : (
								<span className="size-3.5 shrink-0" aria-hidden />
							)}
							<div className="min-w-0 flex-1">
								{edit ? (
									<div className="space-y-1.5">
										<textarea
											// oxlint-disable-next-line jsx-a11y/no-autofocus -- the person just asked to edit it.
											autoFocus
											value={edit.text}
											disabled={edit.saving}
											aria-label={t("composer.queuedEditLabel")}
											onChange={(event) => setEditing({ ...edit, text: event.target.value })}
											onKeyDown={(event) => {
												if (event.key === "Escape") {
													event.preventDefault();
													event.stopPropagation();
													setEditing(undefined);
												} else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
													event.preventDefault();
													void save();
												}
											}}
											rows={Math.min(8, Math.max(2, edit.text.split("\n").length))}
											className="border-input bg-background focus-visible:ring-ring/40 w-full resize-y rounded-md border px-2 py-1.5 text-sm outline-none focus-visible:ring-2"
										/>
										<div className="flex items-center justify-end gap-2">
											<span className="text-muted-foreground mr-auto">
												{t("composer.queuedEditHint")}
											</span>
											<button
												type="button"
												onClick={() => setEditing(undefined)}
												className="hover:bg-accent rounded px-2 py-1"
											>
												{t("composer.queuedEditCancel")}
											</button>
											<button
												type="button"
												disabled={edit.saving}
												onClick={() => void save()}
												className="bg-primary text-primary-foreground rounded px-2 py-1 font-medium disabled:opacity-60"
											>
												{t("composer.queuedEditSave")}
											</button>
										</div>
									</div>
								) : (
									<button
										type="button"
										onClick={() => setOpen(expanded ? undefined : item.id)}
										aria-expanded={expanded}
										className={cn(
											"text-foreground block w-full text-left whitespace-pre-wrap",
											!expanded && "line-clamp-2",
										)}
									>
										{item.text ||
											(item.attachments
												? t("composer.queuedFiles", { count: item.attachments })
												: t("composer.queuedContext"))}
									</button>
								)}
								{item.unconfirmed ? (
									<p className="text-muted-foreground mt-1">{t("composer.queuedUnconfirmed")}</p>
								) : null}
								{item.text && item.attachments ? (
									<p className="text-muted-foreground mt-1">
										{t("composer.queuedFiles", { count: item.attachments })}
									</p>
								) : null}
							</div>
							{edit ? null : (
								<>
									{onEdit && !item.unconfirmed ? (
										<button
											type="button"
											onClick={() => setEditing({ id: item.id, text: item.text })}
											aria-label={t("composer.queuedEdit")}
											title={t("composer.queuedEdit")}
											className={iconButton}
										>
											<Pencil className="size-3.5" />
										</button>
									) : null}
									{onSendNow && !item.unconfirmed && (!item.interrupt || !working) ? (
										<button
											type="button"
											onClick={() => onSendNow(item.id)}
											aria-label={t("composer.sendNow")}
											title={t(working ? "composer.sendNowTitle" : "composer.sendNow")}
											className={cn(iconButton, "hover:text-primary")}
										>
											<Zap className="size-3.5" />
										</button>
									) : null}
									{onUnqueue ? (
										<button
											type="button"
											onClick={() => onUnqueue(item.id)}
											aria-label={t(
												item.unconfirmed ? "composer.dismissUnconfirmed" : "composer.unqueue",
											)}
											title={t(
												item.unconfirmed ? "composer.dismissUnconfirmed" : "composer.unqueue",
											)}
											className={iconButton}
										>
											<X className="size-3.5" />
										</button>
									) : null}
								</>
							)}
						</li>
					);
				})}
			</ul>
			{movable.length > 1 && onReorder ? (
				<p className="text-muted-foreground border-border border-t px-3 py-1.5">
					{t("composer.queuedHint")}
				</p>
			) : null}
		</section>
	);
}
