import * as sdk from "@ahab/plugin/web";
import { WEB_EXPORTS } from "@ahab/plugin/web";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";

import { createContext, resolve } from "@/modules/plugins/runtime/api";
import { webRuntime } from "@/modules/plugins/runtime/components/kit";
import { contributions, removePlugin, reset } from "@/modules/plugins/runtime/components/registry";
import { setHost } from "@/modules/plugins/runtime/model";
import { mockServer } from "@/testing/mock-server";

afterEach(() => reset());

const View = () => null;

describe("what Ahab gives a plugin's web entry", () => {
	it("provides every name the SDK says it does", () => {
		const missing = WEB_EXPORTS.filter((name) => !(name in webRuntime));
		expect(missing).toEqual([]);
	});

	it("lists every name the SDK exports", () => {
		const listed = new Set<string>(WEB_EXPORTS);
		const unlisted = Object.keys(sdk).filter((name) => name !== "WEB_EXPORTS" && !listed.has(name));
		expect(unlisted).toEqual([]);
	});

	it("names commands by their plugin, or the plugin's own", () => {
		expect(resolve("docker", { name: "containers.list" })).toEqual(["docker", "containers.list"]);
		expect(resolve("docker", "kubernetes.pods.list")).toEqual(["kubernetes", "pods.list"]);
		expect(resolve("docker", "ping")).toEqual(["docker", "ping"]);
	});

	it("adds views and windows, and removes all of a plugin's at once", () => {
		const ahab = createContext("echo");
		ahab.views.add({ id: "main", title: "Echo", icon: null, Component: View });
		ahab.activity.add({ id: "open", title: "Open", icon: null, opens: { window: "board" } });
		ahab.windows.add({ kind: "board", title: () => "Board", single: true, Component: View });
		ahab.palette.add({ id: "say", title: "Echo: Say", run: () => {} });
		expect(contributions().views.map((view) => view.key)).toEqual([
			"plugin:echo:main",
			"plugin:echo:open",
		]);
		expect(contributions().palette).toHaveLength(1);

		const openWindow = vi.fn<(tab: unknown) => void>();
		setHost({ openWindow, openSession: () => {}, showView: () => {} });
		ahab.windows.open("board", { x: 1 });
		expect(openWindow).toHaveBeenCalledWith({
			id: "plugin:echo:board",
			component: "plugin",
			title: "Board",
			params: { plugin: "echo", kind: "board", params: { x: 1 } },
		});

		removePlugin("echo");
		expect(contributions().views).toEqual([]);
		expect(contributions().windows).toEqual([]);
	});

	it("runs a command through the API", async () => {
		const asked: unknown[] = [];
		mockServer.use(
			http.post("*/api/v1/plugins/echo/commands/say", async ({ request }) => {
				asked.push(await request.json());
				return HttpResponse.json({ output: { text: "hi" } });
			}),
		);
		const ahab = createContext("echo");
		await expect(
			ahab.commands.call({ name: "say" }, { text: "hi" }, { machine: "m1" }),
		).resolves.toEqual({
			text: "hi",
		});
		expect(asked).toEqual([{ input: { text: "hi" }, machine: "m1" }]);
	});
});
