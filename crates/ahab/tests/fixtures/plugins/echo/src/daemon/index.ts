import { definePlugin, PluginError } from "@ahab/plugin/node";

import {
	delivered,
	fail,
	greet,
	lines,
	notify,
	relay,
	said,
	say,
	slow,
	terminal,
	where,
} from "../shared/index.ts";

/** The greeting the settings ask for. */
function shout(settings: Record<string, unknown>) {
	const greeting = String(settings.greeting);
	return settings.loud ? greeting.toUpperCase() : greeting;
}

export default definePlugin((ahab) => {
	ahab.commands.handle(say, ({ text }) => {
		ahab.events.publish(said, { text });
		return { text, machine: ahab.machine };
	});
	ahab.commands.handle(
		slow,
		({ ms }, call) =>
			new Promise((resolve) => {
				const timer = setTimeout(() => resolve({ cancelled: false }), ms);
				call.signal.addEventListener("abort", () => {
					clearTimeout(timer);
					ahab.log.info("slow was cancelled");
					resolve({ cancelled: true });
				});
			}),
	);
	ahab.commands.handle(where, (_, call) => ({ folder: call.project?.folder ?? null }));
	ahab.commands.handle(relay, ({ text }) => ahab.commands.call("echo.say", { text }));
	ahab.commands.handle(terminal, async (_, call) => {
		if (!call.project) throw new Error("terminal needs a project");
		const started = await ahab.terminals.create({
			project: call.project.id,
			title: "Echo",
			command: "echo",
			args: ["hi there", "it's"],
		});
		const listed = await ahab.terminals.list();
		await ahab.terminals.kill(started.terminal.id);
		return { command: started.terminal.command, listed: listed.length };
	});
	ahab.commands.handle(greet, (_, call) => ({
		machine: shout(ahab.settings()),
		call: shout(call.settings),
	}));
	ahab.onSettings((settings) => ahab.log.info("settings changed", settings));
	ahab.commands.handle(fail, ({ code }) => {
		if (code) throw new PluginError(code, "Echo refused, with a code.");
		throw new Error("Echo was asked to fail.");
	});
	ahab.channels.handle(lines, (channel, { prefix }) => {
		const prefixBytes = new TextEncoder().encode(prefix);
		channel.onClose(() => ahab.log.info("lines closed"));
		channel.onData((data) => {
			if (new TextDecoder().decode(data) === "bye") {
				channel.close("Echo says bye.");
				return;
			}
			const out = new Uint8Array(prefixBytes.length + data.length);
			out.set(prefixBytes, 0);
			out.set(data, prefixBytes.length);
			channel.write(out);
		});
	});
	// A terminal whose context asks for it runs `echo.command` instead.
	ahab.hooks.before("terminals.create", (data) => {
		const wanted = (data.context.echo as { command?: string } | undefined)?.command;
		if (wanted === "deny") return { action: "deny", reason: "Echo says no." };
		return wanted ? { action: "change", data: { ...data, command: wanted } } : { action: "allow" };
	});
	ahab.events.on("echo.said", (data) => ahab.log.info("heard echo.said", data));
	// Notifications: sending, a destination, and the hook.
	const got: { title: string; url: string | null }[] = [];
	ahab.commands.handle(notify, ({ text, kind }) =>
		ahab.notifications.send({
			kind,
			title: text,
			target: { type: "route", path: "/settings/plugins" },
		}),
	);
	ahab.notifications.addDestination({
		id: "memory",
		deliver(notification) {
			got.push({ title: notification.title, url: notification.url });
		},
	});
	ahab.commands.handle(delivered, () => got);
	ahab.hooks.before("notifications.send", (notification) => {
		if (notification.title === "drop me") return { action: "deny", reason: "Echo drops it." };
		if (notification.title.startsWith("shout:"))
			return {
				action: "change",
				data: { ...notification, title: notification.title.slice(6).toUpperCase() },
			};
		return { action: "allow" };
	});
	ahab.log.info("echo is running on", ahab.machine);
});
