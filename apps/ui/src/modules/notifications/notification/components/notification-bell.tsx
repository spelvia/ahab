import { cn } from "cn";
import { Bell } from "lucide-react";
import { useState } from "react";

import { useInbox } from "@/modules/notifications/notification/api";
import { NotificationInbox } from "@/modules/notifications/notification/components/notification-inbox";
import { useTranslation } from "@/shared/i18n";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Sheet, SheetContent } from "@/shared/ui/sheet";

/**
 * The top bar's bell: the unread count on it, and the inbox on a
 * click: a popover under it, or on a phone a sheet the height of the
 * screen. It sits in the top bar because that is where a waiting agent
 * gets your attention.
 */
export function NotificationBell({
	phone = false,
	className,
}: {
	/** Opens the inbox as a full-height sheet, with a 44px target. */
	phone?: boolean;
	/** The trigger's look where it sits; its size above all. */
	className?: string;
}) {
	const { t } = useTranslation("notifications");
	const [open, setOpen] = useState(false);
	const unread = useInbox().data?.unread ?? 0;
	const label = unread > 0 ? t("bell.labelUnread", { count: unread }) : t("bell.label");
	const trigger = (
		<button
			type="button"
			aria-label={label}
			title={label}
			onClick={phone ? () => setOpen(true) : undefined}
			className={cn(
				"hover:bg-accent hover:text-foreground focus-visible:ring-ring data-[state=open]:bg-accent data-[state=open]:text-foreground relative grid shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2",
				unread > 0 ? "text-foreground" : "text-muted-foreground",
				phone ? "size-11 [&>svg]:size-5" : "size-7 [&>svg]:size-4",
				className,
			)}
		>
			<Bell />
			{unread > 0 ? (
				<span
					aria-hidden
					className={cn(
						"bg-primary text-primary-foreground ring-background absolute flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[0.6875rem] leading-none font-bold tabular-nums ring-2",
						phone ? "top-1.5 right-1" : "-top-1 -right-1",
					)}
				>
					{unread > 99 ? "99+" : unread}
				</span>
			) : null}
		</button>
	);
	if (phone) {
		return (
			<>
				{trigger}
				<Sheet open={open} onOpenChange={setOpen}>
					<SheetContent
						title={t("inbox.title")}
						className="h-[calc(100svh-max(0.75rem,env(safe-area-inset-top)))] max-h-none"
					>
						<NotificationInbox sheet onOpen={() => setOpen(false)} />
					</SheetContent>
				</Sheet>
			</>
		);
	}
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>{trigger}</PopoverTrigger>
			<PopoverContent align="end" side="bottom" collisionPadding={8} className="w-auto p-0">
				<NotificationInbox onOpen={() => setOpen(false)} />
			</PopoverContent>
		</Popover>
	);
}
