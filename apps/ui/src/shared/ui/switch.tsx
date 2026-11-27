import { cn } from "cn";
import { Switch as Primitive } from "radix-ui";

/** An on/off switch (Radix), in the app's palette. */
export function Switch({
	checked,
	onCheckedChange,
	disabled,
	label,
	className,
}: {
	checked: boolean;
	onCheckedChange: (checked: boolean) => void;
	disabled?: boolean;
	/** Read by screen readers; the switch shows no text. */
	label: string;
	className?: string;
}) {
	return (
		<Primitive.Root
			checked={checked}
			onCheckedChange={onCheckedChange}
			disabled={disabled}
			aria-label={label}
			className={cn(
				"focus-visible:ring-ring data-[state=checked]:bg-primary data-[state=unchecked]:bg-muted-foreground/30 inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-50",
				className,
			)}
		>
			<Primitive.Thumb className="bg-background pointer-events-none block size-4 rounded-full shadow-sm transition-transform data-[state=checked]:translate-x-4" />
		</Primitive.Root>
	);
}
