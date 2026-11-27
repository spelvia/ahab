import { describe, expect, it } from "vitest";

import {
	lastSeen,
	nameProblem,
	onlineCount,
	platformOf,
	presence,
	statusOf,
	waiting,
} from "@/modules/machines/machine/model";
import type { Machine } from "@/shared/api/generated/ahab.schemas";

function machine(overrides: Partial<Machine> = {}): Machine {
	return {
		id: "0193",
		name: "atlas",
		state: "active",
		isLocal: true,
		online: true,
		checkCode: "K7-4QXM",
		fingerprint: "SHA256:x",
		os: "linux",
		arch: "x86_64",
		version: "0.1.0",
		createdAt: "2026-11-13T09:00:00Z",
		approvedAt: "2026-11-13T09:00:00Z",
		lastSeenAt: "2026-11-13T09:00:00Z",
		...overrides,
	};
}

describe("statusOf", () => {
	it("tells online, offline and pending machines apart", () => {
		expect(statusOf(machine())).toBe("online");
		expect(statusOf(machine({ online: false }))).toBe("offline");
		expect(statusOf(machine({ state: "pending", online: false }))).toBe("pending");
	});
});

describe("onlineCount", () => {
	it("counts the connected machines", () => {
		expect(onlineCount([machine(), machine({ online: false }), machine()])).toBe(2);
	});
});

describe("lastSeen", () => {
	const now = new Date("2026-11-13T12:00:00Z");

	it("says when, in words", () => {
		expect(lastSeen(machine({ lastSeenAt: null }), now)).toBe("never");
		expect(lastSeen(machine({ lastSeenAt: "2026-11-13T11:59:40Z" }), now)).toBe("just now");
		expect(lastSeen(machine({ lastSeenAt: "2026-11-13T11:55:00Z" }), now)).toBe("5 min ago");
		expect(lastSeen(machine({ lastSeenAt: "2026-11-13T09:00:00Z" }), now)).toBe("3 h ago");
		expect(lastSeen(machine({ lastSeenAt: "2026-11-10T09:00:00Z" }), now)).toContain("2026");
	});
});

describe("nameProblem", () => {
	it("accepts hostname-like names", () => {
		expect(nameProblem("build-box-2")).toBeUndefined();
	});

	it("explains what is wrong with other names", () => {
		expect(nameProblem("")).toBe("Enter a name.");
		expect(nameProblem("Build Box")).toBe("Use lowercase letters, digits and hyphens.");
		expect(nameProblem("-box")).toBe("Do not start or end the name with a hyphen.");
		expect(nameProblem("x".repeat(64))).toBe("Use at most 63 characters.");
	});
});

describe("platformOf", () => {
	it("joins what the daemon reported", () => {
		expect(platformOf(machine())).toBe("linux · x86_64");
		expect(platformOf(machine({ os: null, arch: null }))).toBe("");
	});
});

describe("waiting", () => {
	it("lists the machines that wait, oldest first", () => {
		const machines = [
			machine({ id: "a", state: "pending", createdAt: "2026-11-13T10:00:00Z" }),
			machine({ id: "b" }),
			machine({ id: "c", state: "pending", createdAt: "2026-11-13T09:00:00Z" }),
		];
		expect(waiting(machines).map((m) => m.id)).toEqual(["c", "a"]);
	});
});

describe("presence", () => {
	it("says Connected while online, else when it was last seen", () => {
		const now = new Date("2026-11-13T12:00:00Z");
		expect(presence(machine(), now)).toBe("Connected");
		expect(presence(machine({ online: false, lastSeenAt: "2026-11-13T09:00:00Z" }), now)).toBe(
			"3 h ago",
		);
	});
});
