import { usePair } from "@/modules/clients/pairing/api";
import { usePairingRequest } from "@/modules/clients/pairing/hooks";
import { explain } from "@/shared/api/problems";
import { Trans, useTranslation } from "@/shared/i18n";
import { Hero } from "@/shared/ui/hero";
import { CodeEntry, PairingCode } from "@/shared/ui/pairing-code";

/** The problem codes pairing answers, and the key that says each. */
const WORDS = {
	pairing_invalid: "pairing.problems.pairingInvalid",
	too_many_attempts: "pairing.problems.tooManyAttempts",
	insecure_connection: "pairing.problems.insecureConnection",
} as const;

/**
 * Shown whenever the server does not know this browser: Ahab's welcome, the
 * same start page as the desktop app's, with the code of this
 * browser's request to be paired, which a paired client approves, and a
 * field for a code from Add client.
 */
export function PairingScreen({ onPaired }: { onPaired: () => void }) {
	const { t } = useTranslation("clients");
	const { request, stage, error, askAgain } = usePairingRequest(onPaired);
	const pair = usePair();
	const words = Object.fromEntries(Object.entries(WORDS).map(([code, key]) => [code, t(key)]));
	return (
		<Hero title={t("pairing.title")} lead={t("pairing.lead")}>
			<PairingCode
				code={request?.code}
				state={stage}
				approveUrl={
					request ? `${window.location.origin}/settings/clients?approve=${request.code}` : undefined
				}
				problem={explain(error ? new Error(error) : undefined, t("pairing.cannotAsk"), words)}
				onAgain={askAgain}
			>
				<p className="text-muted-foreground text-xs text-pretty">
					<Trans
						t={t}
						i18nKey="pairing.onServer"
						values={{
							approve: `ahab clients approve ${request?.code ?? "…"}`,
							open: "ahab open --no-folder",
						}}
						components={{ code: <code className="text-foreground font-mono" /> }}
					/>
				</p>
			</PairingCode>
			<CodeEntry
				pending={pair.isPending}
				problem={pair.isError ? explain(pair.error, t("pairing.codeFailed"), words) : undefined}
				onSubmit={(code) =>
					pair.mutate({ data: { code, kind: "browser" } }, { onSuccess: onPaired })
				}
			/>
		</Hero>
	);
}
