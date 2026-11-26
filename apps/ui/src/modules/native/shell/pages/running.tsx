import { CheckCircle2, Circle, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { failure, native, type Failure, type LocalAgents } from "@/modules/native/shell/api";
import { Button, Hero, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { useTranslation } from "@/shared/i18n";
import { showSplash } from "@/shared/splash/splash";

type Phase = "start" | "agents" | "open";
type Mark = "waiting" | "active" | "done";

const POLL_MS = 1500;
/** How long to wait for the machine's agents before going on without them. */
const POLLS = 12;

const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

/**
 * Starting Ahab here (or using the one already here), as a checklist that
 * ticks off what really happens: Ahab runs, this computer's machine is
 * there, the agents it found; then Ahab opens.
 */
export function Running({ action }: { action: "run" | "existing" }) {
	const { t } = useTranslation("native");
	const [problem, setProblem] = useState<Failure>();
	const [phase, setPhase] = useState<Phase>("start");
	const [found, setFound] = useState<LocalAgents>();
	const started = useRef(false);
	useEffect(() => {
		if (started.current) return;
		started.current = true;
		void (async () => {
			try {
				const server = action === "run" ? await native.runHere() : await native.existing();
				setPhase("agents");
				let last = "";
				for (let poll = 0; poll < POLLS; poll += 1) {
					try {
						// One question at a time: each answer decides whether to ask again.
						// oxlint-disable-next-line no-await-in-loop
						const now = await native.agentsHere(server.id);
						setFound(now);
						const key = JSON.stringify(now);
						// Done once the machine reports agents twice the same.
						if (now.online && now.agents.length > 0 && key === last) break;
						last = key;
					} catch {
						// The machine may not have connected yet.
					}
					// oxlint-disable-next-line no-await-in-loop
					await wait(POLL_MS);
				}
				setPhase("open");
				await wait(500);
				showSplash(t("splash.opening"));
				await native.openServer(server.id);
			} catch (error) {
				const failed = failure(error);
				if (failed.code === "joined_elsewhere") go("/connect", { address: failed.message });
				else setProblem(failed);
			}
		})();
	}, [action, t]);

	if (problem)
		return (
			<Hero title={action === "run" ? t("running.failed.run") : t("running.failed.existing")}>
				<Problem>{problem.message}</Problem>
				<div className="flex gap-2">
					<Button onClick={() => window.location.reload()}>{t("screens.tryAgain")}</Button>
					<Button variant="outline" onClick={() => go("/welcome")}>
						{t("screens.back")}
					</Button>
				</div>
			</Hero>
		);

	const visible = (found?.agents ?? []).filter((agent) => agent.state !== "notInstalled");
	const looked = phase === "open" || (found?.online === true && found.agents.length > 0);
	return (
		<Hero title={phase === "open" ? t("running.ready") : t("running.title")}>
			<ol aria-label={t("running.steps")} className="w-full max-w-sm space-y-3 text-left">
				<Row mark={phase === "start" ? "active" : "done"}>
					{phase === "start"
						? action === "run"
							? t("running.starting")
							: t("running.connecting")
						: found?.machine
							? t("running.runningOn", { machine: found.machine })
							: t("running.running")}
				</Row>
				<Row
					mark={
						phase === "start" ? "waiting" : found?.online || phase === "open" ? "done" : "active"
					}
				>
					{t("running.machine")}
				</Row>
				<Row mark={phase === "start" ? "waiting" : looked ? "done" : "active"}>
					{looked
						? visible.length > 0
							? t("running.found")
							: t("running.none")
						: t("running.looking")}
					{visible.length > 0 ? (
						<span className="mt-2 flex flex-wrap gap-1.5">
							{visible.map((agent) => (
								<Agent key={agent.agent} label={agent.label} state={agent.state} />
							))}
						</span>
					) : null}
				</Row>
				<Row mark={phase === "open" ? "active" : "waiting"}>{t("running.opening")}</Row>
			</ol>
		</Hero>
	);
}

function Row({ mark, children }: { mark: Mark; children: ReactNode }) {
	const { t } = useTranslation("native");
	return (
		<li
			data-mark={mark}
			className={`flex items-start gap-3 text-sm transition-colors ${mark === "waiting" ? "text-muted-foreground" : "text-heading"}`}
		>
			<span className="mt-0.5 shrink-0" aria-hidden>
				{mark === "done" ? (
					<CheckCircle2 className="text-primary motion-safe:animate-in motion-safe:zoom-in-50 size-4" />
				) : mark === "active" ? (
					<LoaderCircle className="text-primary size-4 animate-spin motion-reduce:animate-none" />
				) : (
					<Circle className="text-muted-foreground/50 size-4" />
				)}
			</span>
			<span className="min-w-0 flex-1">
				<span className="sr-only">{t(`running.mark.${mark}`)}</span>
				{children}
			</span>
		</li>
	);
}

function Agent({ label, state }: { label: string; state: string }) {
	const { t } = useTranslation("native");
	const ready = state === "ready";
	return (
		<span
			className={`motion-safe:animate-in motion-safe:fade-in rounded-md border px-2 py-0.5 text-xs ${ready ? "border-primary/40 bg-primary/10 text-heading" : "text-heading border-amber-500/40 bg-amber-500/10"}`}
		>
			{label}
			<span className="text-muted-foreground ml-1">
				{ready
					? t("running.agent.ready")
					: state === "notSignedIn"
						? t("running.agent.notSignedIn")
						: t("running.agent.needsLook")}
			</span>
		</span>
	);
}
