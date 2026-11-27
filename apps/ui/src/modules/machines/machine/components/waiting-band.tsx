import { Link } from "@tanstack/react-router";
import { useState } from "react";

import { ApproveButton } from "@/modules/machines/enrollment";
import { RemoveConfirmation } from "@/modules/machines/machine/components/remove-confirmation";
import { StatusDot } from "@/modules/machines/machine/components/status-dot";
import { fullTime } from "@/modules/machines/machine/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * The one place that calls for you: machines that wait for approval, each
 * with its check code large enough to compare with the joining computer.
 */
export function WaitingBand({ machines }: { machines: readonly Machine[] }) {
	const { t } = useTranslation("machines");
	if (machines.length === 0) return null;
	return (
		<section
			aria-labelledby="waiting-heading"
			className="border-warning/50 bg-warning/12 rounded-lg border"
		>
			<header className="border-warning/35 flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 border-b px-4 py-3">
				<h2 id="waiting-heading" className="text-warning-foreground text-base font-bold">
					{t("waitingBand.title", { count: machines.length })}
				</h2>
				<p className="text-muted-foreground text-sm">
					<Trans
						t={t}
						i18nKey="waitingBand.hint"
						components={{ code: <code className="font-mono text-xs" /> }}
					/>
				</p>
			</header>
			<ul className="divide-warning/35 divide-y">
				{machines.map((machine) => (
					<WaitingMachine key={machine.id} machine={machine} />
				))}
			</ul>
		</section>
	);
}

function WaitingMachine({ machine }: { machine: Machine }) {
	const { t } = useTranslation("machines");
	const [rejecting, setRejecting] = useState(false);
	return (
		<li className="grid gap-x-6 gap-y-3 px-4 py-4 sm:grid-cols-[1fr_auto] sm:items-center">
			<div className="min-w-0">
				<div className="flex items-center gap-2.5">
					<StatusDot status="pending" />
					<Link
						to="/settings/machines/$id"
						params={{ id: machine.id }}
						className="text-heading font-semibold underline-offset-4 hover:underline"
					>
						{machine.name}
					</Link>
				</div>
				<p className="text-muted-foreground mt-0.5 text-xs" title={fullTime(machine.createdAt)}>
					{t("waitingBand.asked", { when: agoInWords(machine.createdAt) })}
				</p>
				<p className="mt-2 flex items-baseline gap-3">
					<span className="text-muted-foreground text-xs">{t("waitingBand.checkCode")}</span>
					<span className="text-heading font-mono text-2xl font-semibold tracking-[0.12em]">
						{machine.checkCode}
					</span>
				</p>
				<p className="text-muted-foreground truncate font-mono text-xs" title={machine.fingerprint}>
					{machine.fingerprint}
				</p>
			</div>
			<div className="flex flex-wrap items-center gap-2 sm:justify-end">
				{rejecting ? (
					<RemoveConfirmation machine={machine} onCancel={() => setRejecting(false)} />
				) : (
					<>
						<ApproveButton id={machine.id} name={machine.name} />
						<Button variant="ghost" size="sm" onClick={() => setRejecting(true)}>
							{t("waitingBand.reject")}
						</Button>
					</>
				)}
			</div>
		</li>
	);
}
