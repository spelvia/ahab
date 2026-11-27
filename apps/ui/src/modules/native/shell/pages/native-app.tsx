import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { go, useRoute } from "@/modules/native/shell/hooks";
import { Connect } from "@/modules/native/shell/pages/connect";
import { Join } from "@/modules/native/shell/pages/join";
import { LinkScreen } from "@/modules/native/shell/pages/link";
import { Opening } from "@/modules/native/shell/pages/opening";
import { Running } from "@/modules/native/shell/pages/running";
import { Servers } from "@/modules/native/shell/pages/servers";
import { SettingsScreen } from "@/modules/native/shell/pages/settings";
import { Start } from "@/modules/native/shell/pages/start";
import { Welcome } from "@/modules/native/shell/pages/welcome";

/** The built-in screens, by the address after `#`. */
export function NativeApp() {
	const route = useRoute();
	useEffect(() => {
		// An `ahab://` link clicked inside a page of this window.
		const stop = listen<string>("ahab-link", (event) => go("/link", { url: event.payload }));
		return () => {
			void stop.then((unlisten) => unlisten());
		};
	}, []);
	switch (route.path) {
		case "/welcome":
			return <Welcome problem={route.params.get("problem") ?? undefined} />;
		case "/running":
			return <Running action={route.params.get("action") === "existing" ? "existing" : "run"} />;
		case "/connect":
			return (
				<Connect
					address={route.params.get("address") ?? ""}
					machine={route.params.get("machine") === "1"}
				/>
			);
		case "/join":
			return <Join serverId={route.params.get("server") ?? ""} />;
		case "/link":
			return <LinkScreen url={route.params.get("url") ?? undefined} />;
		case "/opening":
			return (
				<Opening
					serverId={route.params.get("server") ?? ""}
					path={route.params.get("path") ?? undefined}
				/>
			);
		case "/servers":
			return <Servers />;
		case "/settings":
			return <SettingsScreen confirmStop={route.params.get("confirm") === "stop"} />;
		default:
			return <Start />;
	}
}
