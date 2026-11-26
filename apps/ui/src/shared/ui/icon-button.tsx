import { cn } from "cn";

/**
 * A small square button with only an icon, named for screen readers and
 * tooltips; for rows and headers. A click stays its own (it does not also
 * open the row it sits in). `pressed` makes it a toggle.
 */
export function IconButton({
	label,
	onClick,
	disabled,
	pressed,
	className,
	children,
}: {
	label: string;
	onClick: () => void;
	disabled?: boolean;
	pressed?: boolean;
	className?: string;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			aria-pressed={pressed}
			title={label}
			disabled={disabled}
			onClick={(event) => {
				event.stopPropagation();
				onClick();
			}}
			className={cn(
				"hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-6 shrink-0 place-items-center rounded outline-none focus-visible:ring-2 disabled:opacity-40 [&_svg]:size-3.5",
				pressed ? "text-primary bg-primary/12" : "text-muted-foreground",
				className,
			)}
		>
			{children}
		</button>
	);
}
