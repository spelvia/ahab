import { LoaderCircle } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { QrCode } from "@/shared/ui/qr-code";

/** Where a request to be paired stands. */
export type PairingState = "asking" | "waiting" | "denied" | "expired" | "failed";

/**
 * A client's request to be paired, the same in the browser, the desktop app
 * and the phone: its code, large, to approve on a client already paired, and
 * the approval page as a QR code with the code in it (`approveUrl`), for a
 * paired phone to scan; it still asks there whether the codes match. An
 * expired request asks again by itself; a denied one says so. `children`
 * adds the ways to approve that this client has (a command, a button).
 */
export function PairingCode({
	code,
	state,
	problem,
	approveUrl,
	onAgain,
	children,
}: {
	code?: string;
	state: PairingState;
	problem?: string;
	approveUrl?: string;
	onAgain: () => void;
	children?: ReactNode;
}) {
	const { t } = useTranslation();
	useEffect(() => {
		if (state === "expired") onAgain();
	}, [state, onAgain]);
	if (state === "denied" || state === "failed")
		return (
			<div className="flex w-full max-w-sm flex-col items-center gap-3">
				<p role={state === "failed" ? "alert" : "status"} className="text-sm text-pretty">
					{state === "denied" ? t("pairing.denied") : (problem ?? t("pairing.cannotAsk"))}
				</p>
				<Button variant="outline" onClick={onAgain}>
					{t("pairing.askAgain")}
				</Button>
			</div>
		);
	const ready = Boolean(code) && state === "waiting";
	return (
		<div className="flex w-full flex-col items-center gap-6 sm:flex-row sm:items-center sm:justify-center sm:gap-10">
			<div className="flex flex-col items-center gap-3">
				<p className="text-muted-foreground text-sm whitespace-nowrap">{t("pairing.approve")}</p>
				<p
					className="text-heading font-mono text-5xl font-semibold tracking-[0.3em] tabular-nums"
					aria-label={t("pairing.code")}
				>
					{code ?? "····"}
				</p>
				<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
					<LoaderCircle className="text-primary size-4 animate-spin motion-reduce:animate-none" />
					{state === "waiting" ? t("pairing.waiting") : t("pairing.asking")}
				</p>
				{children}
			</div>
			{approveUrl && ready ? (
				<figure className="motion-safe:animate-in motion-safe:fade-in hidden flex-col items-center gap-2 motion-safe:duration-300 sm:flex">
					<span className="border-border rounded-lg border bg-white p-1.5 shadow-[0_6px_20px_-10px_rgb(0_0_0/0.5)]">
						<QrCode text={approveUrl} label={t("pairing.qr")} className="size-32" />
					</span>
					<figcaption className="text-muted-foreground max-w-[10rem] text-xs text-pretty">
						{t("pairing.scan")}
					</figcaption>
				</figure>
			) : null}
		</div>
	);
}

/** A code (or link) from Add client on a paired client, typed instead. */
export function CodeEntry({
	onSubmit,
	pending,
	problem,
	children,
}: {
	onSubmit: (code: string) => void;
	pending?: boolean;
	problem?: string;
	children?: ReactNode;
}) {
	const { t } = useTranslation();
	const [code, setCode] = useState("");
	return (
		<form
			className="w-full max-w-sm space-y-2 pt-4"
			onSubmit={(event) => {
				event.preventDefault();
				if (code.trim()) onSubmit(code.trim());
			}}
		>
			<label className="text-muted-foreground block text-sm">
				{t("pairing.typeCode")}
				<span className="mt-1.5 flex gap-2">
					<input
						value={code}
						onChange={(event) => setCode(event.target.value)}
						placeholder="K7QM-4RXT"
						autoComplete="off"
						spellCheck={false}
						autoCapitalize="characters"
						className="bg-background border-input text-foreground focus-visible:ring-ring/50 focus-visible:border-ring block h-9 w-full rounded-md border px-2.5 font-mono text-sm tracking-wider uppercase outline-none focus-visible:ring-[3px]"
					/>
					<Button type="submit" variant="outline" disabled={pending || !code.trim()}>
						{t("pairing.pair")}
					</Button>
				</span>
			</label>
			{problem ? (
				<p role="alert" className="text-destructive text-sm">
					{problem}
				</p>
			) : null}
			{children}
		</form>
	);
}
