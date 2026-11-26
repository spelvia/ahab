import { useEffect, type ReactNode } from "react";

import { hideSplash } from "@/shared/splash/splash";

/** The weave mark, as the title bar shows it: dark ink in light, white in dark. */
export function Mark({ className }: { className?: string }) {
	return (
		<span className={className}>
			<img src="/ahab-mark.svg" alt="" className="h-full w-auto dark:hidden" />
			<img src="/ahab-mark-white.svg" alt="" className="hidden h-full w-auto dark:block" />
		</span>
	);
}

/**
 * A start page, the same in the browser, the desktop app and the phone: the
 * mark, one large headline, and what to do. It rises in once; with reduced
 * motion it is simply there. Drawing it hides the splash.
 */
export function Hero({
	title,
	lead,
	children,
}: {
	title: string;
	lead?: ReactNode;
	children: ReactNode;
}) {
	useEffect(() => hideSplash(), []);
	return (
		<main className="bg-background text-foreground flex min-h-svh items-center justify-center px-6 py-12">
			<section
				aria-labelledby="hero-heading"
				className="motion-safe:animate-in motion-safe:fade-in motion-safe:slide-in-from-bottom-3 w-full max-w-lg text-center motion-safe:duration-300"
			>
				<Mark className="mx-auto mb-9 flex h-12 w-fit" />
				<h1
					id="hero-heading"
					className="font-heading text-heading text-4xl font-extrabold tracking-tight text-balance sm:text-5xl"
				>
					{title}
				</h1>
				{lead ? (
					<p className="text-muted-foreground mx-auto mt-4 max-w-md text-base text-pretty">
						{lead}
					</p>
				) : null}
				<div className="mt-10 flex flex-col items-center gap-4">{children}</div>
			</section>
		</main>
	);
}
