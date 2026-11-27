import { cn } from "cn";
import { Popover as Primitive } from "radix-ui";
import type * as React from "react";

/** A small panel beside a trigger (Radix), in the app's palette. */
export const Popover = Primitive.Root;
export const PopoverAnchor = Primitive.Anchor;
export const PopoverTrigger = Primitive.Trigger;

export function PopoverContent({
	className,
	sideOffset = 6,
	align = "end",
	...props
}: React.ComponentProps<typeof Primitive.Content>) {
	return (
		<Primitive.Portal>
			<Primitive.Content
				sideOffset={sideOffset}
				align={align}
				className={cn(
					"bg-popover text-popover-foreground border-border z-50 w-72 rounded-md border p-3 shadow-[0_8px_24px_-8px_rgb(0_0_0/0.35)] outline-none",
					"data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 duration-150",
					className,
				)}
				{...props}
			/>
		</Primitive.Portal>
	);
}
