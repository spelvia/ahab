import { useState } from "react";

import {
	AddMachineMenu,
	AddMachinePanel,
	AddOverSshPanel,
	readStoredCode,
} from "@/modules/machines/enrollment";
import { useMachines } from "@/modules/machines/machine/api";
import { MachineTable } from "@/modules/machines/machine/components/machine-table";
import { WaitingBand } from "@/modules/machines/machine/components/waiting-band";
import { approved, onlineCount, waiting } from "@/modules/machines/machine/model";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * Settings → Machines: the computers whose daemons connect to this server,
 * at a glance. Each machine's own page holds everything tied to it.
 */
export function MachinesPage() {
	const { t } = useTranslation("machines");
	const { data, isPending, isError, error, refetch, isFetching } = useMachines();
	// A code this tab still keeps reopens its panel after a refresh.
	const [adding, setAdding] = useState<"ssh" | "code" | undefined>(() =>
		readStoredCode() === undefined ? undefined : "code",
	);
	const machines = data?.items ?? [];
	const listed = approved(machines);

	return (
		<section className="space-y-6">
			<header className="flex flex-wrap items-start justify-between gap-4">
				<div className="space-y-1">
					<h1 className="text-2xl font-semibold">
						{t("machinesPage.title")}
						{data && (
							<span className="text-muted-foreground ml-2 text-base font-semibold">
								{t("machinesPage.online", { online: onlineCount(machines), total: listed.length })}
							</span>
						)}
					</h1>
					<p className="text-muted-foreground max-w-3xl text-sm text-pretty">
						{t("machinesPage.description")}
					</p>
				</div>
				{!adding && (
					<AddMachineMenu onSsh={() => setAdding("ssh")} onCode={() => setAdding("code")} />
				)}
			</header>

			{adding === "ssh" && <AddOverSshPanel onClose={() => setAdding(undefined)} />}
			{adding === "code" && <AddMachinePanel onClose={() => setAdding(undefined)} />}

			{isPending && <TableSkeleton />}
			{isError && (
				<div className="flex flex-wrap items-center gap-3" role="alert">
					<p className="text-destructive text-sm">
						{t("machinesPage.loadFailed", { message: error.message })}
					</p>
					<Button variant="outline" size="sm" disabled={isFetching} onClick={() => void refetch()}>
						{t("machinesPage.tryAgain")}
					</Button>
				</div>
			)}
			{data && (
				<>
					<WaitingBand machines={waiting(machines)} />
					{listed.length > 0 ? (
						<MachineTable machines={listed} />
					) : (
						<div className="border-border rounded-lg border border-dashed px-4 py-8 text-center">
							<h2 className="text-base font-bold">{t("machinesPage.emptyTitle")}</h2>
							<p className="text-muted-foreground mx-auto mt-1 max-w-md text-sm">
								<Trans
									t={t}
									i18nKey="machinesPage.emptyText"
									components={{ code: <code className="font-mono text-xs" /> }}
								/>
							</p>
						</div>
					)}
				</>
			)}
		</section>
	);
}

function TableSkeleton() {
	const { t } = useTranslation("machines");
	return (
		<div className="space-y-px" aria-label={t("machinesPage.loading")} role="status">
			{[0, 1, 2].map((row) => (
				<div key={row} className="border-border flex items-center gap-3 border-b px-3 py-4">
					<span className="bg-muted size-2 rounded-full" />
					<span className="bg-muted h-3 w-32 animate-pulse rounded" />
					<span className="bg-muted ml-auto h-3 w-20 animate-pulse rounded" />
				</div>
			))}
		</div>
	);
}
