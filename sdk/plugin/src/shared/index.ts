/**
 * What a plugin declares: its commands, events and channels, as schemas
 * (zod). `shared` holds only these and plain values, so `web` and `daemon`
 * entries can both import it.
 */
import { z } from "zod";

export { z };

/** Where a command runs: on a machine, or next to the server (not supported yet). */
export type CommandTarget = "machine" | "server";

export interface CommandDefinition<Input extends z.ZodType, Output extends z.ZodType> {
	/** Its name in the plugin, such as `containers.list`. */
	name: string;
	target: CommandTarget;
	description?: string;
	input: Input;
	output: Output;
}

/** A command: its name, where it runs, and the shapes of its input and output. */
export function defineCommand<Input extends z.ZodType, Output extends z.ZodType>(
	definition: CommandDefinition<Input, Output>,
): CommandDefinition<Input, Output> {
	return definition;
}

export interface EventDefinition<Data extends z.ZodType> {
	/** Its name in the plugin, such as `container.changed`. */
	name: string;
	data: Data;
	/** Kept while the machine is offline, and sent once it is back. */
	lasting?: boolean;
}

/** An event the plugin publishes. */
export function defineEvent<Data extends z.ZodType>(
	definition: EventDefinition<Data>,
): EventDefinition<Data> {
	return definition;
}

export interface ChannelDefinition<Params extends z.ZodType> {
	/** Its name in the plugin, such as `logs`. */
	name: string;
	params: Params;
}

/** A stream of bytes between a web entry and a daemon entry. */
export function defineChannel<Params extends z.ZodType>(
	definition: ChannelDefinition<Params>,
): ChannelDefinition<Params> {
	return definition;
}

/**
 * A plugin's settings: a zod object, with defaults. Export it as `settings`
 * from `src/shared/settings.ts`; the build writes its JSON Schema to
 * `dist/settings.json`, which the manifest names (`"settings":
 * "dist/settings.json"`). Values are kept per server, machine, project and
 * project, the narrowest winning.
 */
export function defineSettings<Shape extends z.ZodRawShape>(shape: Shape): z.ZodObject<Shape> {
	return z.object(shape);
}

/** The hooks a plugin may intercept, with what they carry. */
export interface Hooks {
	"terminals.create": TerminalCreate;
	/** Before a notification is kept (plugin API 0.1.4): change its text or drop it. */
	"notifications.send": OutgoingNotification;
}

/** What opening a notification shows. */
export type NotificationTarget =
	| { type: "route"; path: string }
	| { type: "url"; url: string }
	| { type: "window"; window: string; props?: Record<string, unknown> }
	| { type: string; [key: string]: unknown };

export type NotificationLevel = "info" | "success" | "warning" | "error";

/**
 * A notification about to be kept, as the `notifications.send` hook sees it.
 * A change may set `title`, `body`, `level` and `target`; `kind` and
 * `source` stay, so Settings still decide.
 */
export interface OutgoingNotification {
	/** `sessions.finished`, `gitlab.pipeline.failed`. */
	kind: string;
	/** `ahab`, or a plugin's ID. */
	source: string;
	level: NotificationLevel;
	title: string;
	body: string | null;
	projectId: string | null;
	target: NotificationTarget;
	groupKey: string | null;
}

/** A kept notification, as destinations and `notification.created` get it. */
export interface DeliveredNotification extends OutgoingNotification {
	id: string;
	createdAt: string;
	/** Where to open it: an absolute link when the server's address is set (`public_url`). */
	url: string | null;
}

/** A terminal about to start on a machine. */
export interface TerminalCreate {
	/** What runs; the login shell when absent. */
	command?: string | null;
	/** Added to its environment. */
	env: Record<string, string>;
	/** The folder it starts in. */
	folder: string;
	/** What the starter said it is for, by plugin: `{ "docker": { "container": "abc" } }`. */
	context: Record<string, unknown>;
}

/** An interceptor's answer. */
export type HookAnswer<Data> =
	| { action: "allow" }
	| { action: "change"; data: Data }
	| { action: "deny"; reason: string };

/**
 * A schema as JSON Schema, for Ahab to show (forms, `ahab run`): as input
 * (fields with defaults are optional) or as output.
 */
export function jsonSchema(schema: z.ZodType, io: "input" | "output" = "output"): unknown {
	try {
		return z.toJSONSchema(schema, { unrepresentable: "any", io });
	} catch {
		return {};
	}
}
