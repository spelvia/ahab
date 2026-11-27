import { cn } from "cn";
import { forwardRef } from "react";

/**
 * How an item of the status bar looks: small, quiet, as tall as the bar,
 * lit on hover. Menus' triggers use it too.
 */
export const statusItemClass =
	"text-muted-foreground hover:text-foreground hover:bg-accent/70 data-[state=open]:bg-accent data-[state=open]:text-foreground focus-visible:ring-ring flex h-full min-w-0 shrink-0 items-center gap-1 px-1.5 whitespace-nowrap outline-none focus-visible:ring-1 focus-visible:ring-inset disabled:hover:bg-transparent disabled:hover:text-muted-foreground";

/** A clickable status bar item; with a title that says what clicking does. */
export const StatusItem = forwardRef<
	HTMLButtonElement,
	React.ButtonHTMLAttributes<HTMLButtonElement> & { label: string }
>(function StatusItem({ label, className, children, ...rest }, ref) {
	return (
		<button
			ref={ref}
			type="button"
			aria-label={label}
			title={label}
			className={cn(statusItemClass, className)}
			{...rest}
		>
			{children}
		</button>
	);
});

/** A status bar item that only shows something. */
export function StatusText({
	title,
	className,
	children,
}: {
	title?: string;
	className?: string;
	children: React.ReactNode;
}) {
	return (
		<span
			title={title}
			className={cn("text-muted-foreground flex h-full shrink-0 items-center px-1.5", className)}
		>
			{children}
		</span>
	);
}
