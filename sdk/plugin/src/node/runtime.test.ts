import { describe, expect, it } from "vitest";

import { defineChannel, defineCommand, z } from "../shared/index.ts";
import { Decoder, encode, type Frame } from "./framing.ts";
import { createRuntime, type Setup } from "./runtime.ts";

describe("framing", () => {
	it("reads frames split anywhere, JSON and channel bytes alike", () => {
		const one = encode({ body: new TextEncoder().encode('{"a":1}') });
		const two = encode({ channel: "c1", body: new Uint8Array([0, 1, 2]) });
		const all = new Uint8Array([...one, ...two]);
		const decoder = new Decoder();
		const frames: Frame[] = [];
		for (let i = 0; i < all.length; i += 3) frames.push(...decoder.push(all.subarray(i, i + 3)));
		expect(frames).toHaveLength(2);
		expect(new TextDecoder().decode(frames[0]!.body)).toBe('{"a":1}');
		expect(frames[1]).toEqual({ channel: "c1", body: new Uint8Array([0, 1, 2]) });
	});

	it("refuses a broken stream", () => {
		expect(() => new Decoder().push(new TextEncoder().encode("Nope: 1\r\n\r\n"))).toThrow(
			/Content-Length/,
		);
	});
});

/** A runtime over an in-memory pipe: what the host would send and see. */
function harness(setup: Setup) {
	let feed!: (chunk: Uint8Array) => void;
	const out: Frame[] = [];
	const decoder = new Decoder();
	let exited: number | undefined;
	createRuntime(setup, {
		write: (bytes) => out.push(...decoder.push(bytes)),
		read: (listener) => {
			feed = listener;
		},
		exit: (code) => {
			exited = code;
		},
	});
	let id = 0;
	const json = () =>
		out
			.filter((frame) => !frame.channel)
			.map((frame) => JSON.parse(new TextDecoder().decode(frame.body)));
	const send = (method: string, params: object = {}, notify = false) => {
		const message = notify
			? { jsonrpc: "2.0", method, params }
			: { jsonrpc: "2.0", id: ++id, method, params };
		feed(encode({ body: new TextEncoder().encode(JSON.stringify(message)) }));
		return notify ? undefined : id;
	};
	const answer = async (sent: number | undefined) => {
		for (let i = 0; i < 50; i++) {
			const found = json().find((message) => message.id === sent && !message.method);
			if (found) return found;
			// oxlint-disable-next-line no-await-in-loop
			await new Promise((resolve) => setTimeout(resolve, 2));
		}
		throw new Error(`no answer to ${sent}`);
	};
	return { send, answer, json, out, feed, exited: () => exited };
}

const say = defineCommand({
	name: "say",
	target: "machine",
	input: z.object({ text: z.string() }),
	output: z.object({ text: z.string() }),
});

describe("a daemon entry", () => {
	it("declares what it handles, after its function ran, and answers commands", async () => {
		const host = harness(async (ahab) => {
			await new Promise((resolve) => setTimeout(resolve, 5));
			ahab.commands.handle(say, ({ text }) => ({ text: `${text} on ${ahab.machine}` }));
		});
		const init = host.send("initialize", { plugin: "t", machine: "atlas", settings: {} });
		// Sent before initialize finished: waits for it.
		const run = host.send("commands.run", { name: "say", input: { text: "hi" } });
		const declared = await host.answer(init);
		expect(declared.result.commands[0]).toMatchObject({ name: "say", input: { type: "object" } });
		expect((await host.answer(run)).result).toEqual({ text: "hi on atlas" });

		const bad = await host.answer(host.send("commands.run", { name: "say", input: {} }));
		expect(bad.error.data.code).toBe("invalid_input");
		const unknown = await host.answer(host.send("commands.run", { name: "nope" }));
		expect(unknown.error.data.code).toBe("command_unknown");
	});

	it("aborts a command when the host cancels it", async () => {
		const wait = defineCommand({
			name: "wait",
			target: "machine",
			input: z.object({}),
			output: z.boolean(),
		});
		const host = harness((ahab) => {
			ahab.commands.handle(
				wait,
				(_, call) =>
					new Promise((resolve) => call.signal.addEventListener("abort", () => resolve(true))),
			);
		});
		await host.answer(host.send("initialize"));
		const run = host.send("commands.run", { name: "wait", input: {} });
		await new Promise((resolve) => setTimeout(resolve, 5));
		host.send("$/cancelRequest", { id: run }, true);
		expect((await host.answer(run)).result).toBe(true);
	});

	it("streams on a channel both ways, and closes it", async () => {
		const lines = defineChannel({ name: "lines", params: z.object({}) });
		let closed = false;
		const host = harness((ahab) => {
			ahab.channels.handle(lines, (channel) => {
				channel.onData((data) => channel.write(new Uint8Array([...data, 33])));
				channel.onClose(() => {
					closed = true;
				});
			});
		});
		await host.answer(host.send("initialize"));
		await host.answer(host.send("channels.open", { name: "lines", channel: "c1", params: {} }));
		host.feed(encode({ channel: "c1", body: new Uint8Array([104, 105]) }));
		expect(host.out.find((frame) => frame.channel === "c1")?.body).toEqual(
			new Uint8Array([104, 105, 33]),
		);
		host.send("channels.close", { channel: "c1" }, true);
		await new Promise((resolve) => setTimeout(resolve, 5));
		expect(closed).toBe(true);
	});

	it("answers hooks, delivers events, and stops", async () => {
		const heard: unknown[] = [];
		let stopped = false;
		const host = harness((ahab) => {
			ahab.hooks.before("terminals.create", (data) => ({
				action: "change",
				data: { ...data, command: "top" },
			}));
			ahab.events.on("terminal.exited", (data) => heard.push(data));
			ahab.onStop(() => {
				stopped = true;
			});
		});
		const declared = await host.answer(host.send("initialize"));
		expect(declared.result).toMatchObject({
			hooks: ["terminals.create"],
			events: ["terminal.exited"],
		});
		const hook = await host.answer(
			host.send("hooks.before", {
				hook: "terminals.create",
				data: { env: {}, folder: "/", context: {} },
			}),
		);
		expect(hook.result).toEqual({
			action: "change",
			data: { env: {}, folder: "/", context: {}, command: "top" },
		});
		host.send("events.deliver", { name: "terminal.exited", data: { code: 0 } }, true);
		await new Promise((resolve) => setTimeout(resolve, 5));
		expect(heard).toEqual([{ code: 0 }]);
		await host.answer(host.send("shutdown"));
		expect(stopped).toBe(true);
		await new Promise((resolve) => setTimeout(resolve, 5));
		expect(host.exited()).toBe(0);
	});

	it("tells Ahab about hooks added or removed after it started", async () => {
		let later!: () => () => void;
		const host = harness((ahab) => {
			later = () => ahab.hooks.before("terminals.create", () => ({ action: "allow" }));
		});
		const declared = await host.answer(host.send("initialize"));
		expect(declared.result.hooks).toEqual([]);
		const remove = later();
		remove();
		const told = host
			.json()
			.filter((message) => message.method === "hooks.declare")
			.map((message) => message.params.hooks);
		expect(told).toEqual([["terminals.create"], []]);
	});

	it("keeps collections of problems and checks on demand", async () => {
		let checked: unknown;
		const host = harness((ahab) => {
			const problems = ahab.diagnostics.collection("gradle");
			ahab.diagnostics.onCheck(async ({ project, roots, reason }) => {
				checked = { roots, reason };
				problems.set(project, "app/Main.java", [
					{ range: { start: { line: 2, character: 4 } }, severity: "warning", message: "old" },
				]);
				problems.delete(project, "app/Old.java");
				problems.clear("w2");
				problems.clear();
			});
			expect(() => ahab.diagnostics.collection("Not OK")).toThrow(/collection name/);
		});
		const declared = await host.answer(host.send("initialize"));
		expect(declared.result.diagnostics).toEqual({ check: true });
		const check = host.send("diagnostics.check", {
			project: { id: "w1", folder: "/w" },
			roots: ["app"],
			reason: "request",
		});
		expect((await host.answer(check)).result).toEqual({});
		expect(checked).toEqual({ roots: ["app"], reason: "request" });
		const sent = host
			.json()
			.filter((message) => String(message.method).startsWith("diagnostics."))
			.map((message) => [message.method, message.params]);
		expect(sent).toEqual([
			[
				"diagnostics.set",
				{
					collection: "gradle",
					project: "w1",
					files: {
						"app/Main.java": [
							{ range: { start: { line: 2, character: 4 } }, severity: "warning", message: "old" },
						],
					},
				},
			],
			["diagnostics.delete", { collection: "gradle", project: "w1", paths: ["app/Old.java"] }],
			["diagnostics.clear", { collection: "gradle", project: "w2" }],
			["diagnostics.clear", { collection: "gradle" }],
		]);
	});

	it("says when a check fails", async () => {
		const host = harness((ahab) => {
			ahab.diagnostics.onCheck(() => {
				throw new Error("gradle is not there");
			});
		});
		await host.answer(host.send("initialize"));
		const failed = await host.answer(
			host.send("diagnostics.check", {
				project: { id: "w", folder: "/w" },
				roots: [],
				reason: "request",
			}),
		);
		expect(failed.error.data.code).toBe("check_failed");
		expect(failed.error.message).toContain("gradle is not there");
	});
});
