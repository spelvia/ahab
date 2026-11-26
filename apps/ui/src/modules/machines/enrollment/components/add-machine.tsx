import { CircleCheck, LoaderCircle, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
	useCreateJoinCode,
	useJoinCodes,
	useWithdrawJoinCode,
} from "@/modules/machines/enrollment/api";
import { type StoredCode, useStoredCode } from "@/modules/machines/enrollment/hooks";
import { formatDate, useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";

type Stage = "creating" | "waiting" | "joined" | "expired" | "failed";

/**
 * A one-time code, inline under the page's header: the command to run on the
 * computer to join, then a visible wait until a machine uses it. The tab
 * keeps the code, so a refresh shows the same one.
 */
export function AddMachinePanel({
	server = globalThis.location?.origin ?? "",
	onClose,
}: {
	server?: string;
	onClose: () => void;
}) {
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	const { stored, keep, forget } = useStoredCode();
	const create = useCreateJoinCode();
	const withdraw = useWithdrawJoinCode();
	const [now, setNow] = useState(() => new Date());
	const asked = useRef(false);

	// A code for this opening, unless the tab already keeps one.
	useEffect(() => {
		if (stored || asked.current) return;
		asked.current = true;
		create.mutate(
			{ data: {} },
			{ onSuccess: (made) => keep({ id: made.id, code: made.code, expiresAt: made.expiresAt }) },
		);
	}, [stored, create, keep]);

	useEffect(() => {
		const timer = setInterval(() => setNow(new Date()), 15_000);
		return () => clearInterval(timer);
	}, []);

	const expired = stored ? new Date(stored.expiresAt) <= now : false;
	const codes = useJoinCodes(expired ? undefined : stored?.id);
	// The code left the list of codes that still work before it expired: a
	// machine used it.
	const joined =
		Boolean(stored) &&
		!expired &&
		codes.data !== undefined &&
		!codes.data.items.some((code) => code.id === stored?.id);

	const stage: Stage = joined
		? "joined"
		: create.isError
			? "failed"
			: !stored
				? "creating"
				: expired
					? "expired"
					: "waiting";

	function close() {
		if (stage === "joined" || stage === "expired") forget();
		onClose();
	}

	function newCode() {
		forget();
		asked.current = false;
	}

	return (
		<section
			aria-labelledby="add-machine-heading"
			className="border-border bg-card rounded-lg border px-4 py-4"
		>
			<header className="flex items-start justify-between gap-4">
				<div>
					<h2 id="add-machine-heading" className="text-base font-bold">
						{t("addMachine.title")}
					</h2>
					<p className="text-muted-foreground mt-1 text-sm">{t("addMachine.intro")}</p>
				</div>
				<Button variant="ghost" size="icon-sm" aria-label={common("actions.close")} onClick={close}>
					<X />
				</Button>
			</header>

			<Command stage={stage} server={server} stored={stored} />

			<div className="mt-3 flex flex-wrap items-center justify-between gap-3">
				<Status stage={stage} stored={stored} error={create.error?.message} />
				{stage === "waiting" && stored && (
					<Button
						variant="ghost"
						size="sm"
						disabled={withdraw.isPending}
						onClick={() => withdraw.mutate({ id: stored.id }, { onSuccess: () => close() })}
					>
						{t("addMachine.withdraw")}
					</Button>
				)}
				{(stage === "joined" || stage === "expired" || stage === "failed") && (
					<Button variant="outline" size="sm" onClick={newCode}>
						{stage === "joined" ? t("addMachine.addAnother") : t("addMachine.newCode")}
					</Button>
				)}
			</div>
		</section>
	);
}

function Command({ stage, server, stored }: { stage: Stage; server: string; stored?: StoredCode }) {
	const { t } = useTranslation("machines");
	const command = stored ? `ahab setup --server ${server} --code ${stored.code}` : "";
	const spent = stage === "joined" || stage === "expired";
	return (
		<div className="bg-background border-border mt-3 flex items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-3">
			<code
				className={
					spent
						? "text-muted-foreground min-h-5 flex-1 font-mono text-sm break-all line-through"
						: "text-heading min-h-5 flex-1 font-mono text-sm break-all"
				}
			>
				{stored ? command : stage === "failed" ? t("addMachine.noCode") : t("addMachine.creating")}
			</code>
			{stored && !spent && <CopyButton text={command} label={t("addMachine.copyLabel")} />}
		</div>
	);
}

function Status({ stage, stored, error }: { stage: Stage; stored?: StoredCode; error?: string }) {
	const { t } = useTranslation("machines");
	const until = stored ? formatDate(stored.expiresAt, { hour: "2-digit", minute: "2-digit" }) : "";
	switch (stage) {
		case "waiting":
			return (
				<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
					<LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" />
					{t("addMachine.waiting", { time: until })}
				</p>
			);
		case "joined":
			return (
				<p className="text-heading flex items-center gap-2 text-sm font-medium" role="status">
					<CircleCheck className="size-4" />
					{t("addMachine.joined")}
				</p>
			);
		case "expired":
			return (
				<p className="text-muted-foreground text-sm" role="status">
					{t("addMachine.expired", { time: until })}
				</p>
			);
		case "failed":
			return (
				<p className="text-destructive text-sm" role="alert">
					{t("addMachine.failed", { message: error })}
				</p>
			);
		default:
			return <span />;
	}
}
