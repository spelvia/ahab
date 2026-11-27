import { useRouterState } from "@tanstack/react-router";
import { X } from "lucide-react";
import { useState } from "react";

import {
	useApprovePairingRequest,
	useDenyPairingRequest,
	usePairingRequests,
} from "@/modules/clients/pairing/api";
import { useClientEvents } from "@/modules/clients/pairing/stream";
import type { PairingRequest } from "@/shared/api/generated/ahab.schemas";
import { usePaired } from "@/shared/api/pairing";
import { explain } from "@/shared/api/problems";
import { Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";

/**
 * A new client asks to be paired: every paired browser shows it, with its
 * name, address and code, to approve or deny. Settings → Clients shows them
 * in its list instead.
 */
export function PairingPrompts() {
	const { t } = useTranslation("clients");
	const paired = usePaired();
	const pathname = useRouterState({ select: (state) => state.location.pathname });
	const onClientsPage = pathname.startsWith("/settings/clients");
	const enabled = paired && pathname !== "/pair";
	useClientEvents(enabled);
	const requests = usePairingRequests(enabled);
	const [hidden, setHidden] = useState<string[]>([]);
	if (!enabled || onClientsPage) return null;
	const shown = (requests.data?.items ?? []).filter((request) => !hidden.includes(request.id));
	if (shown.length === 0) return null;
	return (
		<aside
			aria-label={t("prompts.label")}
			className="fixed right-4 bottom-4 z-50 flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-2"
		>
			{shown.map((request) => (
				<PairingPrompt
					key={request.id}
					request={request}
					onHide={() => setHidden((list) => [...list, request.id])}
				/>
			))}
		</aside>
	);
}

/**
 * One request, with Approve and Deny. Its code is the largest thing on it:
 * approving is comparing it with the code the new client shows (RFC 8628's
 * confirmation), so a code that came in a link or QR code is still checked
 * by eye. `highlight`: the request this page was opened for.
 */
export function PairingPrompt({
	request,
	onHide,
	highlight,
}: {
	request: PairingRequest;
	onHide?: () => void;
	highlight?: boolean;
}) {
	const { t } = useTranslation("clients");
	const approve = useApprovePairingRequest();
	const deny = useDenyPairingRequest();
	const failed = approve.error ?? deny.error;
	const busy = approve.isPending || deny.isPending;
	return (
		<section
			aria-label={t("prompts.requestLabel", { name: request.clientName })}
			className={`bg-popover text-popover-foreground relative rounded-lg border px-4 py-3.5 shadow-[0_8px_24px_-8px_rgb(0_0_0/0.45)] ${highlight ? "border-primary/70 ring-primary/25 ring-2" : "border-border"}`}
		>
			{onHide ? (
				<button
					type="button"
					aria-label={t("prompts.hide")}
					onClick={onHide}
					className="text-muted-foreground hover:text-foreground focus-visible:ring-ring/50 absolute top-2 right-2 rounded p-1 outline-none focus-visible:ring-[3px]"
				>
					<X className="size-3.5" />
				</button>
			) : null}
			<p className="pr-6 text-sm">
				<Trans
					t={t}
					i18nKey={request.address ? "prompts.wantsToPairFrom" : "prompts.wantsToPair"}
					values={{ name: request.clientName, address: request.address }}
					components={{
						name: <span className="text-heading font-semibold" />,
						address: <span className="text-muted-foreground" />,
					}}
				/>
			</p>
			<p
				className="text-heading mt-2 font-mono text-3xl font-semibold tracking-[0.25em] tabular-nums"
				aria-label={t("prompts.codeLabel", { code: request.code })}
			>
				{request.code}
			</p>
			<p className="text-muted-foreground mt-1 text-xs text-pretty">
				{highlight ? t("prompts.checkOpened") : t("prompts.check")}
			</p>
			{failed ? (
				<p role="alert" className="text-destructive mt-2 text-xs">
					{explain(failed, t("prompts.failed"), {
						pairing_not_found: t("prompts.notFound"),
					})}
				</p>
			) : null}
			<div className="mt-3 flex gap-2">
				<Button size="sm" disabled={busy} onClick={() => approve.mutate({ id: request.id })}>
					{t("prompts.approve")}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					disabled={busy}
					onClick={() => deny.mutate({ id: request.id })}
				>
					{t("prompts.deny")}
				</Button>
			</div>
		</section>
	);
}
