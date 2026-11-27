import { usePairingRequests } from "@/modules/clients/pairing/api";
import { PairingPrompt } from "@/modules/clients/pairing/components/pairing-prompts";
import { useTranslation } from "@/shared/i18n";

/**
 * The requests waiting for an answer, on Settings → Clients; the one
 * `ahab login` opened the page for (`highlight`, its code) first.
 */
export function PairingRequests({ highlight }: { highlight?: string }) {
	const { t } = useTranslation("clients");
	const { data } = usePairingRequests();
	const wanted = highlight?.replace(/[-\s]/g, "").toUpperCase();
	const requests = (data?.items ?? []).toSorted(
		(a, b) => Number(b.code === wanted) - Number(a.code === wanted),
	);
	if (requests.length === 0) {
		return wanted ? (
			<p className="text-muted-foreground text-sm" role="status">
				{t("requests.noneWithCode", { code: wanted })}
			</p>
		) : null;
	}
	return (
		<section aria-labelledby="requests-heading" className="space-y-2">
			<h2 id="requests-heading" className="text-base font-bold">
				{t("requests.heading", { count: requests.length })}
			</h2>
			<div className="grid gap-2 md:grid-cols-2">
				{requests.map((request) => (
					<PairingPrompt
						key={request.id}
						request={request}
						highlight={Boolean(wanted) && request.code === wanted}
					/>
				))}
			</div>
		</section>
	);
}
