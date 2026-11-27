import { cn } from "cn";
import type { ReactNode } from "react";

import type { Plugin, PluginMachineState } from "@/shared/api/generated/ahab.schemas";
import { useTranslation } from "@/shared/i18n";

/** A plugin's mark: its first letter on a tile, coloured by its name. */
export function PluginMark({ name, className }: { name: string; className?: string }) {
	const hue = [...name].reduce((sum, char) => sum + char.charCodeAt(0), 0) % 360;
	return (
		<span
			aria-hidden
			className={cn(
				"grid size-9 shrink-0 place-items-center rounded-lg text-sm font-bold text-white",
				className,
			)}
			style={{ background: `oklch(0.58 0.11 ${hue})` }}
		>
			{name.slice(0, 1).toUpperCase()}
		</span>
	);
}

const TONE: Record<PluginMachineState["state"], string> = {
	receiving: "bg-muted-foreground/60",
	starting: "bg-muted-foreground/60",
	running: "bg-success",
	restarting: "bg-warning",
	failed: "bg-destructive",
	unavailable: "bg-warning",
	stopped: "bg-muted-foreground/40",
};

export function StateDot({
	state,
	className,
}: {
	state: PluginMachineState["state"];
	className?: string;
}) {
	return (
		<span
			aria-hidden
			className={cn("inline-block size-2 shrink-0 rounded-full", TONE[state], className)}
		/>
	);
}

/** Where a plugin came from, as a pill. */
export function KindPill({ plugin }: { plugin: Plugin }) {
	const { t } = useTranslation("plugins");
	const kind = plugin.source === "dev" ? "dev" : plugin.source === "git" ? "git" : "local";
	return <span className="pill pill-muted">{t(`kind.${kind}`)}</span>;
}

/** A section of a detail page: a heading, then rows or content. */
export function Section({
	title,
	note,
	action,
	children,
}: {
	title: string;
	note?: ReactNode;
	action?: ReactNode;
	children: ReactNode;
}) {
	const id = `section-${title.replaceAll(" ", "-").toLowerCase()}`;
	return (
		<section aria-labelledby={id} className="space-y-3">
			<div className="flex items-baseline justify-between gap-4">
				<h2 id={id} className="text-base font-bold">
					{title}
				</h2>
				{note ? <p className="text-muted-foreground text-xs">{note}</p> : null}
				{action}
			</div>
			{children}
		</section>
	);
}

/** Rows of label and value, as Ahab's other settings pages show them. */
export function Rows({ children }: { children: ReactNode }) {
	return <dl className="border-border divide-border divide-y border-y text-sm">{children}</dl>;
}

export function Row({
	label,
	hint,
	children,
}: {
	label: ReactNode;
	hint?: ReactNode;
	children: ReactNode;
}) {
	return (
		<div className="grid gap-1 py-2.5 sm:grid-cols-[13rem_1fr] sm:gap-4">
			<dt className="min-w-0">
				<span className="text-muted-foreground">{label}</span>
				{hint ? (
					<span className="text-muted-foreground/80 mt-0.5 block text-xs">{hint}</span>
				) : null}
			</dt>
			<dd className="min-w-0">{children}</dd>
		</div>
	);
}
