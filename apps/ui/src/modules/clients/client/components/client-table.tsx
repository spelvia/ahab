import { Link } from "@tanstack/react-router";

import { ClientActions } from "@/modules/clients/client/components/client-actions";
import { kindLabel } from "@/modules/clients/client/model";
import type { Client } from "@/shared/api/generated/ahab.schemas";
import { agoInWords } from "@/shared/format";
import { formatDate, useTranslation } from "@/shared/i18n";

const chip = "rounded px-1.5 py-0.5 text-[0.6875rem] font-medium whitespace-nowrap";

/** The paired clients: name and marks, kind, last use, where from. */
export function ClientTable({
	clients,
	highlight,
}: {
	clients: readonly Client[];
	highlight?: (client: Client) => boolean;
}) {
	const { t } = useTranslation("clients");
	const names = new Map(clients.map((client) => [client.id, client.name]));
	return (
		<table className="w-full border-collapse text-sm">
			<thead>
				<tr className="border-border text-muted-foreground border-b text-left text-xs font-medium">
					<th scope="col" className="py-2 pr-4 pl-3 font-medium">
						{t("table.columns.client")}
					</th>
					<th scope="col" className="hidden py-2 pr-4 font-medium md:table-cell">
						{t("table.columns.kind")}
					</th>
					<th scope="col" className="py-2 pr-4 font-medium">
						{t("table.columns.lastUsed")}
					</th>
					<th scope="col" className="hidden py-2 pr-4 font-medium lg:table-cell">
						{t("table.columns.paired")}
					</th>
					<th scope="col" className="w-10 py-2 pr-2">
						<span className="sr-only">{t("table.columns.actions")}</span>
					</th>
				</tr>
			</thead>
			<tbody>
				{clients.map((client) => (
					<ClientRow
						key={client.id}
						client={client}
						pairedBy={client.pairedBy ? names.get(client.pairedBy) : undefined}
						highlighted={highlight?.(client) ?? false}
					/>
				))}
			</tbody>
		</table>
	);
}

function ClientRow({
	client,
	pairedBy,
	highlighted,
}: {
	client: Client;
	pairedBy?: string;
	highlighted: boolean;
}) {
	const { t } = useTranslation("clients");
	const local = client.kind === "local";
	return (
		<tr
			className={
				highlighted
					? "border-border bg-warning/10 border-b align-middle"
					: "border-border hover:bg-accent/40 border-b align-middle transition-colors duration-150 last:border-b-0"
			}
		>
			<td className="py-3 pr-4 pl-3">
				<div className="flex flex-wrap items-center gap-1.5">
					<Link
						to="/settings/clients/$id"
						params={{ id: client.id }}
						className="text-heading font-medium underline-offset-4 hover:underline"
					>
						{client.name}
					</Link>
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
				<div className="text-muted-foreground mt-0.5 text-xs md:hidden">
					{kindLabel(client.kind)}
				</div>
			</td>
			<td className="text-muted-foreground hidden py-3 pr-4 md:table-cell">
				{kindLabel(client.kind)}
			</td>
			<td className="text-muted-foreground py-3 pr-4">
				{client.lastUsedAt ? agoInWords(client.lastUsedAt) : t("table.neverUsed")}
				{client.lastAddress ? (
					<div className="text-xs">{t("table.usedFrom", { address: client.lastAddress })}</div>
				) : null}
			</td>
			<td className="text-muted-foreground hidden py-3 pr-4 lg:table-cell">
				{formatDate(client.createdAt, {
					day: "numeric",
					month: "short",
					year: "numeric",
				})}
				{pairedBy ? <div className="text-xs">{t("table.pairedBy", { name: pairedBy })}</div> : null}
			</td>
			<td className="py-3 pr-2 text-right">
				<ClientActions client={client} />
			</td>
		</tr>
	);
}
