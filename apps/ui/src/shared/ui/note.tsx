import { cn } from "cn";

/** A quiet sentence where content would be: loading, empty, or why not. */
export function Note({ className, children }: { className?: string; children: React.ReactNode }) {
	return <p className={cn("text-muted-foreground p-6 text-sm", className)}>{children}</p>;
}
