import { defineChannel, defineCommand, defineEvent, z } from "@ahab/plugin/shared";

export const say = defineCommand({
	name: "say",
	target: "machine",
	description: "Answers the text it is given, with the machine's name.",
	input: z.object({ text: z.string() }),
	output: z.object({ text: z.string(), machine: z.string() }),
});

export const slow = defineCommand({
	name: "slow",
	target: "machine",
	description: "Waits until cancelled, or for `ms` milliseconds.",
	input: z.object({ ms: z.number().default(60_000) }),
	output: z.object({ cancelled: z.boolean() }),
});

export const where = defineCommand({
	name: "where",
	target: "machine",
	description: "Answers the project folder it was run for.",
	input: z.object({}),
	output: z.object({ folder: z.string().nullable() }),
});

export const relay = defineCommand({
	name: "relay",
	target: "machine",
	description: "Asks echo.say, through Ahab, and answers what it said.",
	input: z.object({ text: z.string() }),
	output: z.unknown(),
});

export const terminal = defineCommand({
	name: "terminal",
	target: "machine",
	description: "Starts a terminal in its project, lists its terminals, and kills it.",
	input: z.object({}),
	output: z.object({ command: z.string().nullable(), listed: z.number() }),
});

export const greet = defineCommand({
	name: "greet",
	target: "machine",
	description: "Answers its settings' greeting: the machine's, or the project's when run for one.",
	input: z.object({}),
	output: z.object({ machine: z.string(), call: z.string() }),
});

export const fail = defineCommand({
	name: "fail",
	target: "machine",
	description: "Fails: with `code`, as a PluginError with that code.",
	input: z.object({ code: z.string().optional() }),
	output: z.object({}),
});

export const said = defineEvent({
	name: "said",
	data: z.object({ text: z.string() }),
});

export const lines = defineChannel({
	name: "lines",
	params: z.object({ prefix: z.string().default("") }),
});

export const notify = defineCommand({
	name: "notify",
	target: "machine",
	description:
		"Sends a notification of the kind echo.said: `drop me` is dropped, `shout:` is shouted.",
	input: z.object({ text: z.string(), kind: z.string().default("said") }),
	output: z.object({ id: z.string().nullable() }),
});

export const delivered = defineCommand({
	name: "delivered",
	target: "machine",
	description: "The titles Echo's destination got, oldest first, with their links.",
	input: z.object({}),
	output: z.array(z.object({ title: z.string(), url: z.string().nullable() })),
});
