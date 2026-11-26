import { Button, definePlugin, type WebContext } from "@ahab/plugin/web";
import { useEffect, useState } from "react";

import { lines, say } from "../shared/index.ts";

function EchoView() {
	return <p className="text-muted-foreground px-3 py-2 text-xs">Echo is here.</p>;
}

/** Echo's window: a command, and a channel that answers each line. */
function board(ahab: WebContext) {
	return function Board() {
		const [said, setSaid] = useState("");
		const [heard, setHeard] = useState<{ id: number; line: string }[]>([]);
		const [channel] = useState(() => ahab.channels.open(lines, { prefix: "echo: " }));
		useEffect(() => {
			const stop = channel.onData((bytes) =>
				setHeard((now) => [...now, { id: now.length, line: new TextDecoder().decode(bytes) }]),
			);
			return () => {
				stop();
				channel.close();
			};
		}, [channel]);
		return (
			<div className="flex flex-col gap-2 p-4 text-sm">
				<h2 className="text-heading font-semibold">Echo board</h2>
				<Button
					size="sm"
					onClick={() =>
						void ahab.commands
							.call<{ text: string; machine: string }>(say, { text: "hello" })
							.then((answer) => setSaid(`${answer.text} from ${answer.machine}`))
					}
				>
					Say hello
				</Button>
				{said ? <p data-testid="said">{said}</p> : null}
				<Button size="sm" variant="outline" onClick={() => channel.write("ping")}>
					Send ping
				</Button>
				<ul data-testid="heard">
					{heard.map((item) => (
						<li key={item.id}>{item.line}</li>
					))}
				</ul>
			</div>
		);
	};
}

export default definePlugin((ahab) => {
	ahab.views.add({
		id: "echo",
		title: "Echo",
		icon: <span aria-hidden>⟲</span>,
		Component: EchoView,
	});
	ahab.windows.add({
		kind: "board",
		title: () => "Echo board",
		single: true,
		Component: board(ahab),
	});
	ahab.activity.add({
		id: "board",
		title: "Echo board",
		icon: <span aria-hidden>◎</span>,
		opens: { window: "board" },
	});
	ahab.newWindow.add({ id: "board", title: "Echo board", run: () => ahab.windows.open("board") });
	ahab.palette.add({
		id: "say",
		title: "Echo: Say hello",
		run: () => void ahab.commands.call(say, { text: "hello" }),
	});
});
