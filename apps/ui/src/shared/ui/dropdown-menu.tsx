import { cn } from "cn";
import { ChevronRight } from "lucide-react";
import { DropdownMenu as Primitive } from "radix-ui";
import type * as React from "react";

/** A menu of actions behind a trigger (Radix), in the app's palette. */
export const DropdownMenu = Primitive.Root;
export const DropdownMenuTrigger = Primitive.Trigger;

export function DropdownMenuContent({
	className,
	sideOffset = 4,
	align = "end",
	...props
}: React.ComponentProps<typeof Primitive.Content>) {
	return (
		<Primitive.Portal>
			<Primitive.Content
				sideOffset={sideOffset}
				align={align}
				className={cn(
					"bg-popover text-popover-foreground border-border z-50 min-w-44 overflow-hidden rounded-md border p-1 shadow-[0_8px_24px_-8px_rgb(0_0_0/0.35)]",
					"data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[side=bottom]:slide-in-from-top-1 duration-150",
					className,
				)}
				{...props}
			/>
		</Primitive.Portal>
	);
}

export function DropdownMenuItem({
	className,
	variant = "default",
	...props
}: React.ComponentProps<typeof Primitive.Item> & { variant?: "default" | "destructive" }) {
	return (
		<Primitive.Item
			className={cn(
				"relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none select-none",
				"focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
				"[&_svg]:text-muted-foreground [&_svg]:size-4 [&_svg]:shrink-0",
				variant === "destructive" &&
					"text-destructive focus:bg-destructive/12 focus:text-destructive [&_svg]:text-destructive",
				className,
			)}
			{...props}
		/>
	);
}

export function DropdownMenuSeparator({
	className,
	...props
}: React.ComponentProps<typeof Primitive.Separator>) {
	return <Primitive.Separator className={cn("bg-border -mx-1 my-1 h-px", className)} {...props} />;
}

export const DropdownMenuSub = Primitive.Sub;

export function DropdownMenuSubTrigger({
	className,
	children,
	...props
}: React.ComponentProps<typeof Primitive.SubTrigger>) {
	return (
		<Primitive.SubTrigger
			className={cn(
				"relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none select-none",
				"focus:bg-accent data-[state=open]:bg-accent [&_svg]:text-muted-foreground [&_svg]:size-4 [&_svg]:shrink-0",
				className,
			)}
			{...props}
		>
			{children}
			<ChevronRight className="ml-auto" />
		</Primitive.SubTrigger>
	);
}

export function DropdownMenuSubContent({
	className,
	...props
}: React.ComponentProps<typeof Primitive.SubContent>) {
	return (
		<Primitive.Portal>
			<Primitive.SubContent
				className={cn(
					"bg-popover text-popover-foreground border-border z-50 max-h-80 min-w-44 overflow-y-auto rounded-md border p-1 shadow-[0_8px_24px_-8px_rgb(0_0_0/0.35)]",
					className,
				)}
				{...props}
			/>
		</Primitive.Portal>
	);
}
