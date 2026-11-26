import { useEffect, useRef } from "react";

import { failure, native } from "@/modules/native/shell/api";
import { go } from "@/modules/native/shell/hooks";
import { showSplash } from "@/shared/splash/splash";

/**
 * Where the app starts: the splash, while it decides. The last server
 * when there is one; else the first-launch question, or the Ahab that
 * already runs on this computer.
 */
export function Start() {
	const started = useRef(false);
	useEffect(() => {
		if (started.current) return;
		started.current = true;
		showSplash();
		void (async () => {
			try {
				const boot = await native.boot();
				const { servers, last } = boot.servers;
				const server = servers.find((item) => item.id === last) ?? servers[0];
				if (server) go("/opening", { server: server.id });
				// Ahab already runs here (the command line set it up): use it. An
				// ahab that is installed but not running (never started, or left a
				// server) is no reason to start a server unasked: welcome instead.
				else if (boot.computer.role === "server" && boot.computer.running)
					go("/running", { action: "existing" });
				// This computer is a machine of another server: pair with that one.
				else if (boot.computer.role === "machine")
					go("/connect", { address: boot.computer.joined ?? "" });
				else go("/welcome");
			} catch (error) {
				go("/welcome", { problem: failure(error).message });
			}
		})();
	}, []);
	return null;
}
