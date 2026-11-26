import { describe, expect, it } from "vitest";

import {
	byDay,
	placeOf,
	bySource,
	showing,
	targetPath,
	targetUrl,
} from "@/modules/notifications/notification/model";
import type { Notification as AhabNotification } from "@/shared/api/generated/ahab.schemas";

const session = {
	type: "session",
	projectId: "p1",
	sessionId: "s1",
	chatId: "c1",
};

function at(createdAt: string, id = createdAt): AhabNotification {
	return {
		id,
		kind: "sessions.finished",
		source: "ahab",
		level: "success",
		title: "t",
		target: session,
		createdAt,
	};
}

const kind = (source: string, sourceName: string, id: string) => ({ source, sourceName, id });

describe("notifications", () => {
	it("open the session they are about, and know when it already shows", () => {
		// About one of the session's chats: it opens at that chat's tab.
		expect(targetPath(session)).toBe("/p/p1/s/s1?chat=c1");
		expect(targetPath({ ...session, chatId: undefined })).toBe("/p/p1/s/s1");
		expect(targetPath({ type: "plugin" })).toBeUndefined();
		expect(targetPath(null)).toBeUndefined();
		expect(showing("/p/p1/s/s1", session)).toBe(true);
		expect(showing("/p/p1/s/s2", session)).toBe(false);
	});

	it("open a machine's settings, and only web pages outside Ahab", () => {
		expect(targetPath({ type: "machine", machineId: "m1" })).toBe("/settings/machines/m1");
		expect(targetUrl({ type: "url", url: "https://example.org/r/v1" })).toBe(
			"https://example.org/r/v1",
		);
		expect(targetUrl({ type: "url", url: "javascript:alert(1)" })).toBeUndefined();
		expect(targetUrl({ type: "url", url: "not a url" })).toBeUndefined();
		expect(targetPath({ type: "url", url: "https://example.org" })).toBeUndefined();
		expect(targetPath({ type: "route", path: "/settings/plugins" })).toBe("/settings/plugins");
		expect(targetPath({ type: "route", path: "//evil.example" })).toBeUndefined();
	});

	it("group kinds by where they come from, Ahab first", () => {
		const groups = bySource([
			kind("zeta", "Zeta", "zeta.a"),
			kind("ahab", "Ahab", "sessions.finished"),
			kind("alpha", "Alpha", "alpha.a"),
			kind("ahab", "Ahab", "machines.offline"),
		]);
		expect(groups.map((group) => group.name)).toEqual(["Ahab", "Alpha", "Zeta"]);
		expect(groups[0]?.kinds.map((k) => k.id)).toEqual(["sessions.finished", "machines.offline"]);
	});

	it("are grouped by day, newest first", () => {
		const now = new Date(2026, 10, 18, 12);
		const days = byDay(
			[
				at(new Date(2026, 10, 18, 9).toISOString()),
				at(new Date(2026, 10, 17, 22).toISOString()),
				at(new Date(2026, 10, 2).toISOString()),
			],
			now,
		);
		expect(days.map((day) => day.label).slice(0, 2)).toEqual(["Today", "Yesterday"]);
		expect(days).toHaveLength(3);
	});
});

describe("placeOf", () => {
	it("names the project and session, once", () => {
		const target = {
			type: "session",
			projectName: "ahab",
			sessionTitle: "Fix the test",
		};
		expect(placeOf({ target, body: "May I run make?" })).toBe("ahab · Fix the test");
		expect(placeOf({ target, body: "Fix the test" })).toBe("ahab");
		expect(placeOf({ target: { type: "session" }, body: "Done" })).toBeUndefined();
	});
});
