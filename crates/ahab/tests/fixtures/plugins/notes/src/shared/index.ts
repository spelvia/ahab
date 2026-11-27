import { defineCommand, z } from "@ahab/plugin/shared";

export const report = defineCommand({
	name: "report",
	target: "machine",
	description:
		"Sets a problem with a fix in README.md of the project it runs for, and tries two paths outside it.",
	input: z.object({}),
	output: z.object({ reported: z.boolean() }),
});

export const forget = defineCommand({
	name: "forget",
	target: "machine",
	description: "Forgets README.md's problems.",
	input: z.object({}),
	output: z.object({ forgot: z.boolean() }),
});
