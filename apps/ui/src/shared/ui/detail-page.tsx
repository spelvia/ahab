import { ChevronRight } from "lucide-react";
import { Children, type ReactNode } from "react";

/**
 * The frame every item's page in Settings shares (a project, a machine, a
 * client): a breadcrumb back to its list, its header, then its tabs, each
 * tab its own address. The pages put their own links in; these only look
 * alike.
 */

/** Where the page sits: its list (a link), then the item. */
export function Breadcrumb({ label, children }: { label: string; children: ReactNode }) {
	const parts = Children.toArray(children);
	return (
		<nav aria-label={label} className="text-muted-foreground flex items-center gap-1 text-sm">
			{parts.map((part, index) => (
				// oxlint-disable-next-line no-array-index-key -- a breadcrumb's parts never move
				<span key={index} className="flex items-center gap-1">
					{index > 0 ? <ChevronRight className="size-3.5" aria-hidden /> : null}
					<span className={index === parts.length - 1 ? "text-foreground" : undefined}>{part}</span>
				</span>
			))}
		</nav>
	);
}

/** A breadcrumb's link back to the list. */
export const breadcrumbLink = "hover:text-foreground underline-offset-4 hover:underline";

/** The item's tabs; each a router link given {@link tabLink}'s props. */
export function Tabs({ label, children }: { label: string; children: ReactNode }) {
	return (
		<nav
			aria-label={label}
			className="border-border -mx-1 flex [scrollbar-width:none] gap-1 overflow-x-auto border-b px-1"
		>
			{children}
		</nav>
	);
}

/** A tab's look, active or not, for a router `Link`. */
export const tabLink = {
	className:
		"-mb-px border-b-2 px-2.5 py-2 text-sm font-medium whitespace-nowrap transition-colors duration-150",
	activeProps: { className: "border-primary text-heading" },
	inactiveProps: { className: "text-muted-foreground hover:text-foreground border-transparent" },
} as const;
