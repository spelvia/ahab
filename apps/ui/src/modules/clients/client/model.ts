import type { Client, ClientKind } from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";

/** What a client is, in a word or two. */
export function kindLabel(kind: ClientKind): string {
	return t(`kind.${kind}`, { ns: "clients" });
}

/** The clients to warn about: unused for 3 months and not dismissed. */
export function unusedToMention(
	clients: readonly Client[],
	dismissed: readonly string[],
): Client[] {
	return clients.filter((client) => client.unused && !dismissed.includes(client.id));
}

/** "2 clients haven't been used for 3 months." */
export function unusedNotice(count: number): string {
	return t("unused.notice", { ns: "clients", count });
}
