import { cn } from "cn";
import { X } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import type * as React from "react";

import { useTranslation } from "@/shared/i18n";

export const Dialog = Primitive.Root;
/** @public Part of the component kit. */
export const DialogTrigger = Primitive.Trigger;
/** @public Part of the component kit. */
export const DialogClose = Primitive.Close;

/** How wide a dialog is: `sm` asks for one thing, `md` for a few, `lg` and `xl` show lists or logs. */
export type DialogSize = "sm" | "md" | "lg" | "xl";

const widths: Record<DialogSize, string> = {
	sm: "max-w-[28rem]",
	md: "max-w-[34rem]",
	lg: "max-w-[42rem]",
	xl: "max-w-[60rem]",
};

/**
 * A centred dialog over a dimmed page, closed with Escape or the ✕. Lay it
 * out with `DialogHeader`, `DialogBody` and `DialogFooter`; it brings no
 * padding of its own, so pickers can fill it edge to edge. It never grows
 * past the window: the body scrolls instead.
 */
export function DialogContent({
	className,
	children,
	size = "md",
	closeLabel,
	onOpenAutoFocus,
	...props
}: React.ComponentProps<typeof Primitive.Content> & { size?: DialogSize; closeLabel?: string }) {
	const { t } = useTranslation();
	return (
		<Primitive.Portal>
			<Primitive.Overlay className="data-[state=open]:animate-in data-[state=open]:fade-in-0 fixed inset-0 z-40 bg-black/55 backdrop-blur-[2px]" />
			<Primitive.Content
				className={cn(
					"bg-popover text-popover-foreground border-border data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-[0.98] fixed top-[max(1rem,10vh)] left-1/2 z-50 flex max-h-[calc(100dvh-2rem)] w-[calc(100vw-1.5rem)] -translate-x-1/2 flex-col overflow-hidden rounded-xl border shadow-[0_24px_64px_rgba(0,0,0,0.4)] outline-none sm:max-h-[80vh]",
					widths[size],
					className,
				)}
				// Focus starts in the first field, or on the dialog itself: never
				// on the ✕, which would only invite closing it.
				onOpenAutoFocus={
					onOpenAutoFocus ??
					((event) => {
						event.preventDefault();
						const content = event.currentTarget as HTMLElement;
						const field = content.querySelector<HTMLElement>(
							"[autofocus], input:not([type=checkbox]):not([type=radio]), textarea, select",
						);
						(field ?? content).focus();
					})
				}
				{...props}
			>
				{children}
				<Primitive.Close
					className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring absolute top-4 right-4 rounded-md p-1.5 outline-none focus-visible:ring-2"
					aria-label={closeLabel ?? t("actions.close")}
				>
					<X className="size-4" />
				</Primitive.Close>
			</Primitive.Content>
		</Primitive.Portal>
	);
}

/** The title and what the dialog is for, clear of the ✕. */
export function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			className={cn("flex shrink-0 flex-col gap-1.5 px-6 pt-6 pr-14 pb-4", className)}
			{...props}
		/>
	);
}

/** The dialog's content; it scrolls when the window is short. */
export function DialogBody({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			className={cn("flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-6", className)}
			{...props}
		/>
	);
}

/** The dialog's actions, set apart below its content; the main one goes last. */
export function DialogFooter({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			className={cn(
				"border-border bg-muted/30 flex shrink-0 flex-wrap items-center justify-end gap-2 border-t px-6 py-4",
				className,
			)}
			{...props}
		/>
	);
}

export function DialogTitle({ className, ...props }: React.ComponentProps<typeof Primitive.Title>) {
	return (
		<Primitive.Title
			className={cn("font-heading text-heading text-lg leading-tight font-bold", className)}
			{...props}
		/>
	);
}

export function DialogDescription({
	className,
	...props
}: React.ComponentProps<typeof Primitive.Description>) {
	return (
		<Primitive.Description
			className={cn("text-muted-foreground text-sm leading-relaxed", className)}
			{...props}
		/>
	);
}
