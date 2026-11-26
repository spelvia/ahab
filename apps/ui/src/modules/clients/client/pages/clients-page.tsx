import { KeyRound, Plus } from "lucide-react";
import { useState } from "react";

import { useClients, useRevokeClient } from "@/modules/clients/client/api";
import { ClientTable } from "@/modules/clients/client/components/client-table";
import { TokenDialog } from "@/modules/clients/client/components/token-dialog";
import { unusedNotice, unusedToMention } from "@/modules/clients/client/model";
import { AddClientDialog, PairingRequests } from "@/modules/clients/pairing";
import { formatList, useTranslation } from "@/shared/i18n";
import { dismissedUnusedClients, rememberUnusedClientsDismissed } from "@/shared/preferences/store";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";

/**
 * Settings → Clients: every browser, phone, command line and token paired
 * with this server, the requests waiting for an answer, and the way to add
 * one. `approve` is the code `ahab login` opened this page for.
 */
export function ClientsPage({ approve }: { approve?: string }) {
	const { t } = useTranslation("clients");
	const { data, isPending, isError, error, refetch, isFetching } = useClients();
	const [adding, setAdding] = useState(false);
	const [token, setToken] = useState(false);
	const [reviewing, setReviewing] = useState(false);
	const [revokingAll, setRevokingAll] = useState(false);
	const [dismissed, setDismissed] = useState(dismissedUnusedClients);
	const revoke = useRevokeClient();
	const clients = data?.items ?? [];
	const unused = unusedToMention(clients, dismissed);
	const allUnused = clients.filter((client) => client.unused);

	async function revokeAllUnused() {
		await Promise.all(allUnused.map((client) => revoke.mutateAsync({ id: client.id })));
		setRevokingAll(false);
		setReviewing(false);
	}

	return (
		<section className="space-y-6">
			<header className="flex flex-wrap items-start justify-between gap-4">
				<div className="space-y-1">
					<h1 className="text-2xl font-semibold">
						{t("page.title")}
						{data && (
							<span className="text-muted-foreground ml-2 text-base font-semibold">
								{t("page.paired", { count: clients.length })}
							</span>
						)}
					</h1>
					<p className="text-muted-foreground max-w-3xl text-sm text-pretty">
						{t("page.description")}
					</p>
				</div>
				<div className="flex gap-2">
					<Button variant="outline" onClick={() => setToken(true)}>
						<KeyRound />
						{t("page.createToken")}
					</Button>
					<Button onClick={() => setAdding(true)}>
						<Plus />
						{t("page.addClient")}
					</Button>
				</div>
			</header>

			<PairingRequests highlight={approve} />

			{unused.length > 0 ? (
				<div
					role="status"
					className="border-border bg-muted/40 flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3"
				>
					<p className="text-sm">{unusedNotice(unused.length)}</p>
					<div className="flex gap-2">
						<Button variant="ghost" size="sm" onClick={() => setReviewing((on) => !on)}>
							{reviewing ? t("unused.showAll") : t("unused.review")}
						</Button>
						<Button variant="outline" size="sm" onClick={() => setRevokingAll(true)}>
							{t("unused.revokeAll")}
						</Button>
						<Button
							variant="ghost"
							size="sm"
							onClick={() => {
								const ids = allUnused.map((client) => client.id);
								rememberUnusedClientsDismissed(ids);
								setDismissed(ids);
								setReviewing(false);
							}}
						>
							{t("unused.dismiss")}
						</Button>
					</div>
				</div>
			) : null}

			{isPending && <p className="text-muted-foreground text-sm">{t("page.loading")}</p>}
			{isError && (
				<div className="flex flex-wrap items-center gap-3" role="alert">
					<p className="text-destructive text-sm">
						{t("page.loadError", { message: error.message })}
					</p>
					<Button variant="outline" size="sm" disabled={isFetching} onClick={() => void refetch()}>
						{t("page.tryAgain")}
					</Button>
				</div>
			)}
			{data && (
				<ClientTable
					clients={reviewing ? allUnused : clients}
					highlight={(client) => reviewing && client.unused}
				/>
			)}

			<AddClientDialog open={adding} onOpenChange={setAdding} />
			<TokenDialog open={token} onOpenChange={setToken} />
			<ConfirmDialog
				open={revokingAll}
				onOpenChange={setRevokingAll}
				title={t("unused.confirm.title", { count: allUnused.length })}
				description={t("unused.confirm.description", {
					names: formatList(
						allUnused.map((client) => client.name),
						"unit",
					),
				})}
				action={t("unused.confirm.action")}
				destructive
				pending={revoke.isPending}
				error={revoke.error?.message}
				onConfirm={() => void revokeAllUnused()}
			/>
		</section>
	);
}
