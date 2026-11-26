/**
 * A daemon entry's SDK: `definePlugin` runs the plugin over the process's
 * pipes, as Ahab's daemon starts it.
 */
import { createRuntime, type Io, type Setup } from "./runtime.ts";

export type {
	CallContext,
	Channel,
	CheckRequest,
	Destination,
	Diagnostic,
	DiagnosticCollection,
	Fix,
	Io,
	NodeContext,
	Position,
	SendNotification,
	Setup,
	Severity,
	TerminalInfo,
	ProjectRef,
} from "./runtime.ts";
export { createRuntime, PluginError, PROTOCOL_VERSION } from "./runtime.ts";
export * from "../shared/index.ts";

/** The process's own pipes: stdin and stdout carry the protocol. */
function processIo(): Io {
	return {
		write: (bytes) => void process.stdout.write(bytes),
		read(listener, end) {
			process.stdin.on("data", (chunk: Buffer) => listener(new Uint8Array(chunk)));
			process.stdin.on("end", end);
		},
		exit: (code) => process.exit(code),
	};
}

let started = false;

/**
 * A daemon entry: `export default definePlugin((ahab) => { … })`. Starts
 * speaking with Ahab over stdin and stdout, so the console goes to Ahab's
 * logs for the plugin instead.
 */
export function definePlugin(setup: Setup): Setup {
	if (started) return setup;
	started = true;
	const { log } = createRuntime(setup, processIo());
	// stdout is the protocol: the console goes to Ahab's logs instead.
	/* oxlint-disable no-console */
	console.log = (...parts: unknown[]) => log("info", parts);
	console.info = console.log;
	console.debug = console.log;
	console.warn = (...parts: unknown[]) => log("warn", parts);
	console.error = (...parts: unknown[]) => log("error", parts);
	/* oxlint-enable no-console */
	return setup;
}
