import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import { BellOff, CheckCircle2, CircleAlert, Info, Settings2, X, XCircle } from "lucide-react";

import {
	useClearRead,
	useInbox,
	useMarkAllRead,
	useMarkRead,
} from "@/modules/notifications/notification/api";
import { useOpenTarget } from "@/modules/notifications/notification/hooks";
import { byDay, placeOf, timeOf } from "@/modules/notifications/notification/model";
import type { Notification as AhabNotification } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { SheetClose } from "@/shared/ui/sheet";

const ICONS = {
	success: <CheckCircle2 className="text-primary size-4" />,
	warning: <CircleAlert className="size-4 text-amber-500" />,
	error: <XCircle className="text-destructive size-4" />,
	info: <Info className="text-muted-foreground size-4" />,
} as const;

/**
 * The inbox: newest first, by day; opening one marks it read.
 * `sheet` lays it out for a phone's sheet, which titles it: the whole
 * height, 44px targets, and a way to close it.
 */
export function NotificationInbox({
	onOpen,
	sheet = false,
}: {
	onOpen?: () => void;
	sheet?: boolean;
}) {
	const { t } = useTranslation(["notifications", "common"]);
	const inbox = useInbox();
	const read = useMarkRead();
	const readAll = useMarkAllRead();
	const clear = useClearRead();
	const openTarget = useOpenTarget();
	const items = inbox.data?.items ?? [];
	const unread = inbox.data?.unread ?? 0;
	const open = (notification: AhabNotification) => {
		if (!notification.readAt) read.mutate({ id: notification.id });
		openTarget(notification.target);
		onOpen?.();
	};
	return (
		<section
			aria-label={t("inbox.label")}
			className={cn(
				"flex flex-col",
				sheet ? "min-h-0 flex-1" : "max-h-[min(32rem,70vh)] w-[min(24rem,calc(100vw-2rem))]",
			)}
		>
			<header
				className={cn(
					"border-border flex items-center gap-2 border-b",
					sheet ? "px-3 pb-2" : "px-3 py-2",
				)}
			>
				{sheet ? (
					<p className="text-muted-foreground min-w-0 flex-1 truncate px-2 text-sm whitespace-nowrap">
						{unread > 0 ? t("inbox.unreadCount", { count: unread }) : null}
					</p>
				) : (
					<h2 className="text-heading flex-1 text-sm font-semibold">
						{t("inbox.title")}
						{unread > 0 ? (
							<span className="text-muted-foreground font-normal">
								{" "}
								{t("inbox.unread", { count: unread })}
							</span>
						) : null}
					</h2>
				)}
				<Button
					size={sheet ? "sm" : "xs"}
					className={sheet ? "h-11" : undefined}
					variant="ghost"
					disabled={unread === 0 || readAll.isPending}
					onClick={() => readAll.mutate({ params: {} })}
				>
					{t("inbox.markAllRead")}
				</Button>
				<Button
					size={sheet ? "sm" : "xs"}
					className={sheet ? "h-11" : undefined}
					variant="ghost"
					disabled={items.every((item) => !item.readAt) || clear.isPending}
					onClick={() => clear.mutate()}
				>
					{t("inbox.clearRead")}
				</Button>
				<Link
					to="/settings/notifications"
					onClick={onOpen}
					aria-label={t("inbox.settings")}
					title={t("inbox.settings")}
					className={cn(
						"text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid place-items-center rounded-md outline-none focus-visible:ring-2",
						sheet ? "size-11" : "size-6",
					)}
				>
					<Settings2 className="size-4" />
				</Link>
				{sheet ? (
					<SheetClose
						aria-label={t("common:actions.close")}
						title={t("common:actions.close")}
						className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-11 place-items-center rounded-md outline-none focus-visible:ring-2"
					>
						<X className="size-5" />
					</SheetClose>
				) : null}
			</header>
			<div className="min-h-0 flex-1 overflow-y-auto">
				{inbox.isPending ? (
					<p className="text-muted-foreground px-3 py-6 text-center text-sm">
						{t("common:state.loading")}
					</p>
				) : items.length === 0 ? (
					<div className="flex flex-col items-center gap-2 px-6 py-10 text-center">
						<BellOff className="text-muted-foreground size-5" />
						<p className="text-heading text-sm font-medium">{t("inbox.emptyTitle")}</p>
						<p className="text-muted-foreground text-xs text-pretty">
							{t("inbox.emptyDescription")}
						</p>
					</div>
				) : (
					byDay(items).map((day) => (
						<div key={day.label}>
							<p className="text-muted-foreground px-3 pt-3 pb-1 text-[0.6875rem] font-medium tracking-wide uppercase">
								{day.label}
							</p>
							<ul>
								{day.items.map((item) => (
									<li key={item.id}>
										<button
											type="button"
											onClick={() => open(item)}
											className={cn(
												"hover:bg-accent/60 focus-visible:bg-accent/60 flex w-full items-start gap-2.5 px-3 text-left outline-none",
												sheet ? "min-h-14 py-3" : "py-2",
											)}
										>
											<span className="mt-0.5 shrink-0">{ICONS[item.level] ?? ICONS.info}</span>
											<span className="min-w-0 flex-1">
												<span
													className={`block truncate text-sm ${item.readAt ? "text-muted-foreground" : "text-heading font-semibold"}`}
												>
													{item.title}
												</span>
												{item.body ? (
													<span className="text-muted-foreground line-clamp-2 block text-xs text-pretty">
														{item.body}
													</span>
												) : null}
												{placeOf(item) ? (
													<span className="text-muted-foreground/80 mt-0.5 block truncate text-[0.6875rem]">
														{placeOf(item)}
													</span>
												) : null}
											</span>
											<span className="text-muted-foreground shrink-0 text-[0.6875rem] tabular-nums">
												{timeOf(item.createdAt)}
											</span>
											{item.readAt ? null : (
												<span
													aria-label={t("inbox.unreadMark")}
													className="bg-primary mt-1.5 size-1.5 shrink-0 rounded-full"
												/>
											)}
										</button>
									</li>
								))}
							</ul>
						</div>
					))
				)}
			</div>
		</section>
	);
}
