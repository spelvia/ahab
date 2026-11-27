import type * as React from "react";

import { usePhone } from "@/shared/media";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Sheet, SheetContent, SheetTrigger } from "@/shared/ui/sheet";

/**
 * Details a tap away: a popover beside its trigger on wider screens, a
 * sheet from the bottom on phones, where a popover would cover the
 * keyboard's neighbourhood and be hard to reach.
 */
export function PopoverSheet({
	title,
	label,
	triggerClassName,
	trigger,
	contentClassName,
	open,
	onOpenChange,
	children,
}: {
	/** The sheet's heading on phones; the popover's accessible name elsewhere. */
	title: string;
	/** The trigger's accessible name and tooltip. */
	label: string;
	triggerClassName?: string;
	trigger: React.ReactNode;
	/** For the popover only: its width and padding. */
	contentClassName?: string;
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
	children: React.ReactNode;
}) {
	const phone = usePhone();
	if (phone)
		return (
			<Sheet open={open} onOpenChange={onOpenChange}>
				<SheetTrigger aria-label={label} title={label} className={triggerClassName}>
					{trigger}
				</SheetTrigger>
				<SheetContent title={title}>
					<div className="min-h-0 overflow-y-auto px-5 pb-2">{children}</div>
				</SheetContent>
			</Sheet>
		);
	return (
		<Popover open={open} onOpenChange={onOpenChange}>
			<PopoverTrigger aria-label={label} title={label} className={triggerClassName}>
				{trigger}
			</PopoverTrigger>
			<PopoverContent align="end" aria-label={title} className={contentClassName}>
				{children}
			</PopoverContent>
		</Popover>
	);
}
