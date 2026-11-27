import { cn } from "cn";
import { Dialog as Primitive } from "radix-ui";
import type * as React from "react";

export const Sheet = Primitive.Root;
export const SheetTrigger = Primitive.Trigger;

/** A panel that slides up from the bottom of a phone screen. */
export function SheetContent({
	className,
	title,
	children,
	...props
}: React.ComponentProps<typeof Primitive.Content> & { title: string }) {
	return (
		<Primitive.Portal>
			<Primitive.Overlay className="data-[state=open]:animate-in data-[state=open]:fade-in-0 fixed inset-0 z-40 bg-black/50" />
			<Primitive.Content
				aria-describedby={undefined}
				className={cn(
					"bg-popover text-popover-foreground border-border data-[state=open]:animate-in data-[state=open]:slide-in-from-bottom fixed inset-x-0 bottom-0 z-50 flex max-h-[80svh] flex-col rounded-t-2xl border-t pb-[max(1rem,env(safe-area-inset-bottom))] outline-none",
					className,
				)}
				{...props}
			>
				<div className="bg-muted-foreground/40 mx-auto mt-2.5 h-1 w-10 rounded-full" aria-hidden />
				<Primitive.Title className="text-heading px-5 pt-4 pb-2 text-base font-semibold">
					{title}
				</Primitive.Title>
				{children}
			</Primitive.Content>
		</Primitive.Portal>
	);
}

export const SheetClose = Primitive.Close;
