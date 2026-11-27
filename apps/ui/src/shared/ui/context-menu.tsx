import { cn } from "cn";
import { ContextMenu as Primitive } from "radix-ui";
import type * as React from "react";

/** Actions on a right-click (Radix), styled as the dropdown menus. */
export const ContextMenu = Primitive.Root;
export const ContextMenuTrigger = Primitive.Trigger;

export function ContextMenuContent({
	className,
	...props
}: React.ComponentProps<typeof Primitive.Content>) {
	return (
		<Primitive.Portal>
			<Primitive.Content
				className={cn(
					"bg-popover text-popover-foreground border-border z-50 min-w-44 overflow-hidden rounded-md border p-1 shadow-[0_8px_24px_-8px_rgb(0_0_0/0.35)]",
					className,
				)}
				{...props}
			/>
		</Primitive.Portal>
	);
}

export function ContextMenuItem({
	className,
	...props
}: React.ComponentProps<typeof Primitive.Item>) {
	return (
		<Primitive.Item
			className={cn(
				"relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none select-none",
				"focus:bg-accent focus:text-accent-foreground [&_svg]:text-muted-foreground [&_svg]:size-4 [&_svg]:shrink-0",
				"data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
				className,
			)}
			{...props}
		/>
	);
}

export function ContextMenuSeparator({
	className,
	...props
}: React.ComponentProps<typeof Primitive.Separator>) {
	return <Primitive.Separator className={cn("bg-border -mx-1 my-1 h-px", className)} {...props} />;
}
