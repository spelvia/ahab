import { cn } from "cn";
import type { ReactNode } from "react";

import { useTranslation } from "@/shared/i18n";

/**
 * One section of a machine's page: a heading, a line on what it covers, an
 * action on the right, and its content. `planned` marks a section whose
 * content is not built yet.
 */
export function MachineSection({
	id,
	title,
	description,
	action,
	planned = false,
	children,
}: {
	id: string;
	title: string;
	description?: ReactNode;
	action?: ReactNode;
	planned?: boolean;
	children: ReactNode;
}) {
	const { t } = useTranslation("machines");
	return (
		<section id={id} aria-labelledby={`${id}-heading`} className="scroll-mt-6 space-y-3">
			<div className="flex items-end justify-between gap-4">
				<div className="min-w-0 space-y-0.5">
					<h2 id={`${id}-heading`} className="flex items-center gap-2 text-base font-bold">
						{title}
						{planned && <span className="pill pill-muted">{t("section.planned")}</span>}
					</h2>
					{description && (
						<p className="text-muted-foreground text-sm text-pretty">{description}</p>
					)}
				</div>
				{action}
			</div>
			{children}
		</section>
	);
}

/** Rows under a hairline, as every section lists its things. */
export function SectionRows({ children, className }: { children: ReactNode; className?: string }) {
	return (
		<ul className={cn("border-border divide-border divide-y border-y text-sm", className)}>
			{children}
		</ul>
	);
}

/**
 * Something a machine will show once its feature is built: what it is, and
 * what Ahab will say about it, greyed out.
 */
export function PlannedRow({
	icon,
	title,
	detail,
	source,
}: {
	icon?: ReactNode;
	title: string;
	detail: string;
	/** A short note beside it, such as when it comes. */
	source?: string;
}) {
	return (
		<li className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-0.5 py-2.5 sm:grid-cols-[12rem_minmax(0,1fr)_auto]">
			<span className="text-muted-foreground flex items-center gap-2 font-medium [&_svg]:size-4">
				{icon}
				{title}
			</span>
			<span className="text-muted-foreground/80 col-span-2 row-start-2 text-xs sm:col-span-1 sm:row-start-auto">
				{detail}
			</span>
			<span className="text-muted-foreground/70 col-start-2 row-start-1 text-[0.6875rem] sm:col-start-auto sm:row-start-auto">
				{source}
			</span>
		</li>
	);
}

/** A label and its value, for the machine's system and identity. */
export function Field({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="grid gap-1 py-2 sm:grid-cols-[12rem_1fr] sm:gap-4">
			<dt className="text-muted-foreground">{label}</dt>
			<dd className="min-w-0">{children}</dd>
		</div>
	);
}

/** An empty section's one line. */
export function Empty({ children }: { children: ReactNode }) {
	return <p className="border-border text-muted-foreground border-y py-3 text-sm">{children}</p>;
}
