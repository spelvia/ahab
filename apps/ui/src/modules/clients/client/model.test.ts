import { describe, expect, it } from "vitest";

import { unusedNotice, unusedToMention } from "@/modules/clients/client/model";
import type { Client } from "@/shared/api/generated/ahab.schemas";

const client = (id: string, unused: boolean): Client => ({
	id,
	name: id,
	kind: "browser",
	pairedVia: "link",
	createdAt: "2026-08-01T09:00:00Z",
	encrypted: true,
	current: false,
	unused,
});

describe("unused clients", () => {
	it("mentions the unused ones not dismissed yet", () => {
		const clients = [client("a", true), client("b", true), client("c", false)];
		expect(unusedToMention(clients, ["a"]).map((d) => d.id)).toEqual(["b"]);
		expect(unusedNotice(2)).toBe("2 clients haven't been used for 3 months. Revoke them?");
		expect(unusedNotice(1)).toBe("1 client hasn't been used for 3 months. Revoke it?");
	});
});
