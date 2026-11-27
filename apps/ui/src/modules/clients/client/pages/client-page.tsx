import { Link, useNavigate } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { useClients } from "@/modules/clients/client/api";
import { ClientActions } from "@/modules/clients/client/components/client-actions";
import { kindLabel } from "@/modules/clients/client/model";
import type { Client } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatDate, useTranslation } from "@/shared/i18n";
import { Breadcrumb, breadcrumbLink } from "@/shared/ui/detail-page";

/** A date and time, as the page shows it. */
function date(value: string): string {
	return formatDate(value, {
		day: "numeric",
		month: "short",
		year: "numeric",
		hour: "numeric",
		minute: "2-digit",
	});
}

const chip = "rounded px-1.5 py-0.5 text-[0.6875rem] font-medium whitespace-nowrap";

/**
 * Settings → Clients → a client, framed as a project or a machine is: its
 * name, marks and ⋯ menu (Rename, Revoke), then how and when it was paired
 * and last used.
 */
export function ClientPage({ clientId }: { clientId: string }) {
	const { t } = useTranslation(["clients", "common"]);
	const clients = useClients();
	const list = clients.data?.items ?? [];
	const client = list.find((item) => item.id === clientId);
	return (
		<section className="max-w-3xl space-y-6">
			<Breadcrumb label={t("clientPage.breadcrumb")}>
				<Link to="/settings/clients" className={breadcrumbLink}>
					{t("clientPage.clients")}
				</Link>
				{client?.name ?? t("clientPage.client")}
			</Breadcrumb>
			{clients.isPending ? (
				<p className="text-muted-foreground text-sm">{t("common:state.loading")}</p>
			) : clients.isError ? (
				<p role="alert" className="text-destructive text-sm">
					{clients.error.message}
				</p>
			) : client ? (
				<Details client={client} pairedBy={list.find((item) => item.id === client.pairedBy)} />
			) : (
				<p className="text-muted-foreground text-sm">{t("clientPage.notFound")}</p>
			)}
		</section>
	);
}

function Details({ client, pairedBy }: { client: Client; pairedBy?: Client }) {
	const { t } = useTranslation("clients");
	const navigate = useNavigate();
	const local = client.kind === "local";
	return (
		<>
			<header className="flex items-start justify-between gap-4">
				<div className="min-w-0 space-y-1">
					<div className="flex flex-wrap items-center gap-2">
						<h1 className="font-heading text-heading min-w-0 truncate text-2xl font-semibold">
							{client.name}
						</h1>
						{client.current ? (
							<span className={`${chip} bg-accent text-heading`}>{t("table.thisClient")}</span>
						) : null}
						{!client.encrypted && !local ? (
							<span
								className={`${chip} bg-warning/15 text-warning-foreground`}
								title={t("table.notEncryptedHint")}
							>
								{t("table.notEncrypted")}
							</span>
						) : null}
						{client.unused ? (
							<span className={`${chip} bg-muted text-muted-foreground`}>{t("table.unused")}</span>
						) : null}
					</div>
					<p className="text-muted-foreground text-sm">
						{kindLabel(client.kind)} ·{" "}
						{client.lastUsedAt
							? t("clientPage.lastUsedAgo", { ago: agoInWords(client.lastUsedAt) })
							: t("clientPage.neverUsed")}
					</p>
				</div>
				<ClientActions
					client={client}
					onRevoked={() => void navigate({ to: "/settings/clients" })}
				/>
			</header>
			<dl className="border-border divide-border divide-y border-y text-sm">
				<Row label={t("clientPage.kind")}>{kindLabel(client.kind)}</Row>
				<Row label={t("clientPage.paired")}>
					{date(client.createdAt)}
					{" · "}
					{t(`clientPage.via.${client.pairedVia}`)}
					{pairedBy ? (
						<>
							{" · "}
							<Link
								to="/settings/clients/$id"
								params={{ id: pairedBy.id }}
								className="underline-offset-4 hover:underline"
							>
								{t("table.pairedBy", { name: pairedBy.name })}
							</Link>
						</>
					) : null}
				</Row>
				<Row label={t("clientPage.lastUsed")}>
					{client.lastUsedAt ? date(client.lastUsedAt) : t("table.neverUsed")}
					{client.lastAddress ? ` · ${t("table.usedFrom", { address: client.lastAddress })}` : null}
				</Row>
				{local ? null : (
					<Row label={t("clientPage.connection")}>
						{client.encrypted ? t("clientPage.encrypted") : t("table.notEncryptedHint")}
					</Row>
				)}
				{client.userAgent ? (
					<Row label={t("clientPage.userAgent")}>
						<span className="font-mono text-xs break-all">{client.userAgent}</span>
					</Row>
				) : null}
			</dl>
		</>
	);
}

function Row({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="flex flex-col gap-1 py-2.5 sm:flex-row sm:gap-4">
			<dt className="w-36 shrink-0 font-medium">{label}</dt>
			<dd className="text-muted-foreground min-w-0">{children}</dd>
		</div>
	);
}
