import { cn } from "cn";
import { X } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import type * as React from "react";

import { useTranslation } from "@/shared/i18n";

export const Drawer = Primitive.Root;

/**
 * A panel along the right edge over a dimmed page, for one item's details
 * beside its list; the whole screen on phones. Closed with Escape or ✕.
 */
export function DrawerContent({
	className,
	title,
	description,
	header,
	children,
	...props
}: React.ComponentProps<typeof Primitive.Content> & {
	title: string;
	description?: string;
	/** Shown before the title, such as a mark. */
	header?: React.ReactNode;
}) {
	const { t } = useTranslation();
	return (
		<Primitive.Portal>
			<Primitive.Overlay className="data-[state=open]:animate-in data-[state=open]:fade-in-0 fixed inset-0 z-40 bg-black/40" />
			<Primitive.Content
				className={cn(
					"bg-popover text-popover-foreground border-border data-[state=open]:animate-in data-[state=open]:slide-in-from-right fixed inset-0 z-50 flex flex-col outline-none duration-200 sm:inset-y-0 sm:right-0 sm:left-auto sm:w-[34rem] sm:max-w-[calc(100vw-2rem)] sm:border-l sm:shadow-[-16px_0_48px_rgba(0,0,0,0.25)]",
					className,
				)}
				{...(description ? {} : { "aria-describedby": undefined })}
				{...props}
			>
				<div className="border-border flex items-start gap-3 border-b px-5 py-4">
					{header}
					<div className="min-w-0 flex-1">
						<Primitive.Title className="text-heading truncate text-base font-semibold">
							{title}
						</Primitive.Title>
						{description && (
							<Primitive.Description className="text-muted-foreground text-xs">
								{description}
							</Primitive.Description>
						)}
					</div>
					<Primitive.Close
						className="text-muted-foreground hover:text-foreground focus-visible:ring-ring rounded-md p-1 outline-none focus-visible:ring-2"
						aria-label={t("actions.close")}
					>
						<X className="size-4" />
					</Primitive.Close>
				</div>
				<div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
			</Primitive.Content>
		</Primitive.Portal>
	);
}
