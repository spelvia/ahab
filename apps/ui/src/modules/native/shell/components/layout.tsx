import { useEffect, type ReactNode } from "react";

import { useTranslation } from "@/shared/i18n";
import { hideSplash } from "@/shared/splash/splash";
import { Button } from "@/shared/ui/button";
import { Hero, Mark } from "@/shared/ui/hero";

/** A built-in screen: the mark, a heading, and what it holds. Drawing it hides the splash. */
export function Page({
	title,
	lead,
	children,
	wide,
}: {
	title: string;
	lead?: ReactNode;
	children: ReactNode;
	wide?: boolean;
}) {
	useEffect(() => hideSplash(), []);
	return (
		<main className="bg-background text-foreground flex min-h-svh items-start justify-center px-5 pt-[10vh] pb-12">
			<section
				aria-labelledby="page-heading"
				className={wide ? "w-full max-w-2xl" : "w-full max-w-md"}
			>
				<Mark className="mb-7 block h-7" />
				<header className="mb-6 space-y-1.5">
					<h1
						id="page-heading"
						className="font-heading text-heading text-2xl font-semibold text-pretty"
					>
						{title}
					</h1>
					{lead ? <p className="text-muted-foreground text-sm text-pretty">{lead}</p> : null}
				</header>
				<div className="space-y-5">{children}</div>
			</section>
		</main>
	);
}

/** A bordered group of content. */
export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
	return (
		<div className={`border-border bg-card space-y-3 rounded-lg border px-4 py-4 ${className}`}>
			{children}
		</div>
	);
}

/** What went wrong, read out at once. */
export function Problem({ children }: { children: ReactNode }) {
	return (
		<p role="alert" className="text-destructive text-sm text-pretty">
			{children}
		</p>
	);
}

/** Asks before a change that stops something: what stops, then Yes or No. */
export function Confirm({
	question,
	detail,
	yes,
	onYes,
	onNo,
	destructive,
}: {
	question: string;
	detail?: ReactNode;
	yes: string;
	onYes: () => void;
	onNo: () => void;
	destructive?: boolean;
}) {
	const { t } = useTranslation();
	return (
		<div
			role="alertdialog"
			aria-label={question}
			className="border-border bg-muted/40 space-y-3 rounded-lg border p-4"
		>
			<p className="text-heading text-sm font-semibold">{question}</p>
			{detail ? <p className="text-muted-foreground text-sm text-pretty">{detail}</p> : null}
			<div className="flex gap-2">
				<Button size="sm" variant={destructive ? "destructive" : "default"} onClick={onYes}>
					{yes}
				</Button>
				<Button size="sm" variant="outline" onClick={onNo}>
					{t("actions.cancel")}
				</Button>
			</div>
		</div>
	);
}

export { Button, Hero };
