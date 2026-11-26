import { Link, useNavigate } from "@tanstack/react-router";
import { type ReactNode, useState } from "react";

import { ApproveButton } from "@/modules/machines/enrollment";
import { useMachine } from "@/modules/machines/machine/api";
import { MachineMenu } from "@/modules/machines/machine/components/machine-menu";
import { RemoveConfirmation } from "@/modules/machines/machine/components/remove-confirmation";
import { RenameForm } from "@/modules/machines/machine/components/rename-form";
import { StatusDot } from "@/modules/machines/machine/components/status-dot";
import { fullTime, platformOf, presence, statusOf } from "@/modules/machines/machine/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";
import { formatList, Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { Breadcrumb, breadcrumbLink, Tabs, tabLink } from "@/shared/ui/detail-page";

/**
 * A machine's tabs, each its own address under `/settings/machines/<id>`;
 * their labels are `machineLayout.tabs.<label>`.
 */
const TABS = [
	{ to: "/settings/machines/$id", label: "overview", exact: true },
	{ to: "/settings/machines/$id/agents", label: "agents" },
	{ to: "/settings/machines/$id/lsps", label: "lsps" },
	{ to: "/settings/machines/$id/resources", label: "resources" },
	{ to: "/settings/machines/$id/plugins", label: "plugins" },
	{ to: "/settings/machines/$id/environment", label: "environment" },
	{ to: "/settings/machines/$id/settings", label: "settings" },
] as const;

/**
 * One machine: its name, status and actions, then its tabs and the open
 * tab (`children`). Where `ahab setup`'s link leads: a machine that waits
 * for approval shows its check code instead of tabs.
 */
export function MachineLayout({ id, children }: { id: string; children: ReactNode }) {
	const { t } = useTranslation("machines");
	const { data: machine, isPending, isError, error, refetch, isFetching } = useMachine(id);

	return (
		<section className="space-y-6">
			<Breadcrumb label={t("machineLayout.breadcrumb")}>
				<Link to="/settings/machines" className={breadcrumbLink}>
					{t("machineLayout.machines")}
				</Link>
				{machine?.name ?? t("machineLayout.machine")}
			</Breadcrumb>

			{isPending && <p className="text-muted-foreground text-sm">{t("machineLayout.loading")}</p>}
			{isError &&
				(error.status === 404 ? (
					<div className="space-y-2">
						<p className="text-muted-foreground text-sm">{t("machineLayout.notFound")}</p>
						<Link
							to="/settings/machines"
							className="text-sm font-medium underline underline-offset-4"
						>
							{t("machineLayout.back")}
						</Link>
					</div>
				) : (
					<div className="flex flex-wrap items-center gap-3" role="alert">
						<p className="text-destructive text-sm">
							{t("machineLayout.loadFailed", { message: error.message })}
						</p>
						<Button
							variant="outline"
							size="sm"
							disabled={isFetching}
							onClick={() => void refetch()}
						>
							{t("machineLayout.tryAgain")}
						</Button>
					</div>
				))}

			{machine && <Loaded machine={machine}>{children}</Loaded>}
		</section>
	);
}

function Loaded({ machine, children }: { machine: Machine; children: ReactNode }) {
	const { t } = useTranslation("machines");
	const [mode, setMode] = useState<"view" | "rename" | "remove">("view");
	const navigate = useNavigate();
	const pending = machine.state === "pending";
	const platform = platformOf(machine);

	return (
		<>
			<header className="space-y-4">
				<div className="flex flex-wrap items-start justify-between gap-4">
					{mode === "rename" ? (
						<RenameForm machine={machine} onDone={() => setMode("view")} />
					) : (
						<div className="min-w-0 space-y-1">
							<div className="flex flex-wrap items-center gap-3">
								<StatusDot status={statusOf(machine)} className="size-3" />
								<h1 className="text-2xl font-semibold">{machine.name}</h1>
								{machine.isLocal && (
									<span className="pill pill-muted">{t("machineLayout.thisServer")}</span>
								)}
								{machine.outdated && (
									<span
										className="pill pill-warn"
										title={t("machineLayout.olderAhabTitle", {
											methods: formatList(machine.missingMethods ?? [], "unit"),
										})}
									>
										{t("machineLayout.olderAhab")}
									</span>
								)}
							</div>
							<p className="text-muted-foreground pl-6 text-sm">
								{[platform, machine.version && `Ahab ${machine.version}`]
									.filter(Boolean)
									.join(" · ")}
								{(platform || machine.version) && " · "}
								<span
									className={machine.online ? "text-foreground" : undefined}
									title={fullTime(machine.lastSeenAt)}
								>
									{presence(machine)}
								</span>
							</p>
						</div>
					)}
					<MachineMenu
						machine={machine}
						onRename={() => setMode("rename")}
						onRemove={() => setMode("remove")}
					/>
				</div>

				{mode === "remove" && (
					<div className="border-destructive/30 bg-destructive/6 rounded-lg border px-4 py-3">
						<RemoveConfirmation
							machine={machine}
							onCancel={() => setMode("view")}
							onRemoved={() => void navigate({ to: "/settings/machines" })}
						/>
					</div>
				)}

				{pending && mode !== "remove" && (
					<Waiting machine={machine} onReject={() => setMode("remove")} />
				)}

				{!pending && (
					<Tabs label={t("machineLayout.tabsLabel", { name: machine.name })}>
						{TABS.map((tab) => (
							<Link
								key={tab.to}
								to={tab.to}
								params={{ id: machine.id }}
								activeOptions={{ exact: "exact" in tab }}
								{...tabLink}
							>
								{t(`machineLayout.tabs.${tab.label}`)}
							</Link>
						))}
					</Tabs>
				)}
			</header>

			{!pending && children}
		</>
	);
}

function Waiting({ machine, onReject }: { machine: Machine; onReject: () => void }) {
	const { t } = useTranslation("machines");
	return (
		<div className="border-warning/50 bg-warning/12 grid gap-4 rounded-lg border px-4 py-4 sm:grid-cols-[1fr_auto] sm:items-center">
			<div>
				<p className="text-sm">
					<Trans
						t={t}
						i18nKey="machineLayout.waiting"
						components={{ code: <code className="font-mono text-xs" /> }}
					/>
				</p>
				<p className="text-heading mt-3 font-mono text-4xl font-semibold tracking-[0.14em]">
					{machine.checkCode}
				</p>
				<p
					className="text-muted-foreground mt-1 truncate font-mono text-xs"
					title={machine.fingerprint}
				>
					{machine.fingerprint}
				</p>
			</div>
			<div className="flex items-center gap-2">
				<ApproveButton id={machine.id} name={machine.name} />
				<Button variant="ghost" size="sm" onClick={onReject}>
					{t("machineLayout.reject")}
				</Button>
			</div>
		</div>
	);
}
