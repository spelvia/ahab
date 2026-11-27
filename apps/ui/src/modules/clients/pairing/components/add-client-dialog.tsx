import { CircleCheck, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
	useCreatePairing,
	usePairingLink,
	useWithdrawPairing,
} from "@/modules/clients/pairing/api";
import { type PairingUsed, useClientEvents } from "@/modules/clients/pairing/stream";
import type { CreatedPairing } from "@/shared/api/generated/ahab.schemas";
import { formatDate, Trans, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/shared/ui/dialog";
import { QrCode } from "@/shared/ui/qr-code";
import { toast } from "@/shared/ui/toast";

/**
 * Add client: a QR code, the link it holds and a code to type, for a new
 * client to pair with. They work once, for 10 minutes; the dialog closes
 * when the new client is paired.
 */
export function AddClientDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("clients");
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent size="md">
				<DialogHeader>
					<DialogTitle>{t("addClient.title")}</DialogTitle>
					<DialogDescription>{t("addClient.description")}</DialogDescription>
				</DialogHeader>
				<DialogBody>
					{/* A new link each time it opens. */}
					{open ? <Offer onPaired={() => onOpenChange(false)} /> : null}
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}

function Offer({ onPaired }: { onPaired: () => void }) {
	const { t } = useTranslation("clients");
	const create = useCreatePairing();
	const withdraw = useWithdrawPairing();
	const [offered, setOffered] = useState<CreatedPairing>();
	const asked = useRef(false);
	const done = useRef(false);
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 15_000);
		return () => clearInterval(timer);
	}, []);

	useEffect(() => {
		if (asked.current) return;
		asked.current = true;
		create.mutate({ data: {} }, { onSuccess: setOffered });
	}, [create]);

	const link = usePairingLink(offered?.id);
	const state = link.data?.state;

	function paired(name?: string) {
		if (done.current) return;
		done.current = true;
		toast(name ? t("addClient.pairedName", { name }) : t("addClient.paired"));
		onPaired();
	}
	useClientEvents(offered !== undefined, (used: PairingUsed) => {
		if (used.pairing === offered?.id) paired(used.name);
	});
	useEffect(() => {
		if (state === "used") paired();
	});

	// Closed before it was used: the link stops working.
	const latest = useRef({ offered, state, withdraw: withdraw.mutate });
	useEffect(() => {
		latest.current = { offered, state, withdraw: withdraw.mutate };
	});
	useEffect(
		() => () => {
			const { offered: was, state: last, withdraw: withdrawLink } = latest.current;
			if (was && !done.current && last !== "used") withdrawLink({ id: was.id });
		},
		[],
	);

	if (create.isError) {
		return (
			<p role="alert" className="text-destructive text-sm">
				{t("addClient.failed", { message: create.error.message })}
			</p>
		);
	}
	if (!offered) {
		return (
			<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
				<LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" />
				{t("addClient.making")}
			</p>
		);
	}
	const expired = state === "expired" || Date.parse(offered.expiresAt) <= now;
	const until = formatDate(offered.expiresAt, {
		hour: "2-digit",
		minute: "2-digit",
	});
	return (
		<div className="space-y-4">
			<div className="bg-background/40 border-border flex justify-center rounded-lg border py-5">
				<QrCode text={offered.url} label={t("addClient.qrLabel")} />
			</div>
			<div className="bg-background border-border flex items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-3">
				<code className="text-heading min-h-5 flex-1 truncate font-mono text-xs">
					{offered.url}
				</code>
				<CopyButton text={offered.url} label={t("addClient.copyLabel")} />
			</div>
			{offered.code ? (
				<p className="text-muted-foreground text-sm">
					<Trans
						t={t}
						i18nKey="addClient.orType"
						values={{ origin: globalThis.location?.origin ?? "Ahab", code: offered.code }}
						components={{
							code: (
								<span className="text-heading font-mono text-base font-semibold tracking-wider" />
							),
						}}
					/>
				</p>
			) : null}
			{expired ? (
				<div className="flex items-center justify-between gap-3">
					<p className="text-muted-foreground text-sm">{t("addClient.expired", { time: until })}</p>
					<Button
						variant="outline"
						size="sm"
						onClick={() => {
							setOffered(undefined);
							create.mutate({ data: {} }, { onSuccess: setOffered });
						}}
					>
						{t("addClient.again")}
					</Button>
				</div>
			) : state === "used" ? (
				<p className="text-heading flex items-center gap-2 text-sm font-medium" role="status">
					<CircleCheck className="size-4" />
					{t("addClient.done")}
				</p>
			) : (
				<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
					<LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" />
					{t("addClient.waiting", { time: until })}
				</p>
			)}
		</div>
	);
}
