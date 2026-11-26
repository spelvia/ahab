import { LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { failure, native, type Reached, type Server } from "@/modules/native/shell/api";
import { Button, Hero, Page, Problem } from "@/modules/native/shell/components/layout";
import { go } from "@/modules/native/shell/hooks";
import { t as translate, Trans, useTranslation } from "@/shared/i18n";
import { showSplash } from "@/shared/splash/splash";
import { CodeEntry, PairingCode } from "@/shared/ui/pairing-code";

const field =
	"border-input bg-background focus-visible:ring-ring/50 focus-visible:border-ring h-9 w-full rounded-md border px-3 text-sm outline-none focus-visible:ring-[3px]";

/** After pairing: join as a machine when asked, else open the server. */
async function afterPairing(server: Server, machine: boolean) {
	if (machine) {
		go("/join", { server: server.id });
		return;
	}
	showSplash(translate("splash.connecting", { ns: "native", name: server.name }));
	await native.openServer(server.id);
}

/**
 * Connect to an Ahab server: its address, then pairing.
 * The app asks and shows its code, to approve from a paired client; or it
 * takes a code or link from Add client.
 */
export function Connect({ address, machine }: { address: string; machine: boolean }) {
	const { t } = useTranslation(["native", "common"]);
	const [input, setInput] = useState(address);
	const [reached, setReached] = useState<Reached>();
	const [problem, setProblem] = useState<string>();
	const [busy, setBusy] = useState(false);
	// A new request: the pairing panel starts again.
	const [attempt, setAttempt] = useState(0);

	const reach = useCallback(async (text: string) => {
		setProblem(undefined);
		setBusy(true);
		try {
			setReached(await native.reach(text));
		} catch (error) {
			setProblem(failure(error).message);
		} finally {
			setBusy(false);
		}
	}, []);

	// An address given (a link, a machine's server) is tried at once.
	const tried = useRef(false);
	useEffect(() => {
		if (address && !tried.current) {
			tried.current = true;
			void reach(address);
		}
	}, [address, reach]);

	if (reached?.known) {
		const known = reached.known;
		return (
			<Page title={t("connect.known.title", { name: known.name })} lead={known.url}>
				<div className="flex gap-2">
					<Button onClick={() => void afterPairing(known, machine)}>
						{t("connect.known.open")}
					</Button>
					<Button variant="outline" onClick={() => setReached({ ...reached, known: null })}>
						{t("screens.pairAgain")}
					</Button>
				</div>
			</Page>
		);
	}
	if (reached)
		return (
			<Pairing
				key={attempt}
				reached={reached}
				machine={machine}
				onBack={() => setReached(undefined)}
				onAgain={() => setAttempt((n) => n + 1)}
			/>
		);

	return (
		<Page title={t("connect.title")} lead={t("connect.lead")}>
			<form
				onSubmit={(event) => {
					event.preventDefault();
					void reach(input);
				}}
				className="space-y-3"
			>
				<label className="block space-y-1.5">
					<span className="text-sm font-medium">{t("connect.address")}</span>
					<input
						className={field}
						value={input}
						onChange={(event) => setInput(event.target.value)}
						placeholder="https://ahab.example"
						autoFocus
						spellCheck={false}
						autoCapitalize="off"
					/>
				</label>
				{problem ? <Problem>{problem}</Problem> : null}
				<div className="flex gap-2">
					<Button type="submit" disabled={busy || !input.trim()}>
						{busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : null}
						{t("connect.continue")}
					</Button>
					<Button type="button" variant="ghost" onClick={() => go("/servers")}>
						{t("common:actions.cancel")}
					</Button>
				</div>
			</form>
		</Page>
	);
}

/** The code to approve, and a field for a code or link from Add client. */
function Pairing({
	reached,
	machine,
	onBack,
	onAgain,
}: {
	reached: Reached;
	machine: boolean;
	onBack: () => void;
	onAgain: () => void;
}) {
	const { t } = useTranslation("native");
	const [code, setCode] = useState<string>();
	const [state, setState] = useState<"asking" | "waiting" | "denied" | "expired" | "failed">(
		"asking",
	);
	const [problem, setProblem] = useState<string>();
	const done = useRef(false);

	useEffect(() => {
		const run = new AbortController();
		void (async () => {
			try {
				setState("asking");
				const asked = await native.ask(reached.url);
				if (run.signal.aborted) return;
				setCode(asked.code);
				setState("waiting");
				// Each wait is a long poll; ask again until answered.
				const poll = async (): Promise<void> => {
					const waited = await native.wait(asked.id);
					if (run.signal.aborted || done.current) return;
					if (waited.state === "paired") {
						done.current = true;
						await afterPairing(waited.server, machine);
					} else if (waited.state === "waiting") {
						await poll();
					} else {
						setState(waited.state);
					}
				};
				await poll();
			} catch (error) {
				if (!run.signal.aborted) {
					setProblem(failure(error).message);
					setState("failed");
				}
			}
		})();
		return () => run.abort();
	}, [reached.url, machine]);

	const pairWithTyped = async (text: string) => {
		setProblem(undefined);
		try {
			let server: Server;
			if (/^(ahab|https?):/i.test(text)) {
				const link = await native.parseLink(text);
				if (link.kind !== "pair") throw { code: "bad_link", message: t("pairing.notPairingLink") };
				server = await native.pair(link.server, link.secret, link.code);
			} else {
				server = await native.pair(reached.url, undefined, text);
			}
			done.current = true;
			await afterPairing(server, machine);
		} catch (error) {
			setProblem(failure(error).message);
		}
	};

	const host = new URL(reached.url).host;
	return (
		<Hero
			title={t("pairing.title", { host })}
			lead={t("pairing.lead", { version: reached.version, url: reached.url })}
		>
			<PairingCode
				code={code}
				state={state}
				problem={state === "failed" ? problem : undefined}
				approveUrl={code ? `${reached.url}/settings/clients?approve=${code}` : undefined}
				onAgain={onAgain}
			>
				<Button
					variant="outline"
					size="sm"
					onClick={() => void native.openInBrowser(`${reached.url}/settings/clients`)}
				>
					{t("pairing.openApproval")}
				</Button>
				<p className="text-muted-foreground text-xs">
					<Trans
						t={t}
						i18nKey="pairing.onServer"
						values={{ command: `ahab clients approve ${code ?? "…"}` }}
						components={{ code: <code className="text-foreground font-mono" /> }}
					/>
				</p>
			</PairingCode>
			<CodeEntry
				problem={state !== "failed" ? problem : undefined}
				onSubmit={(text) => void pairWithTyped(text)}
			>
				<Button type="button" variant="ghost" size="sm" onClick={onBack}>
					{t("screens.back")}
				</Button>
			</CodeEntry>
		</Hero>
	);
}
