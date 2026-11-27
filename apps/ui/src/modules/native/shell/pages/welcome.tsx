import { useEffect, useState } from "react";

import { native, type ThisComputer } from "@/modules/native/shell/api";
import { Button, Hero, Problem } from "@/modules/native/shell/components/layout";
import { go, isPhoneApp } from "@/modules/native/shell/hooks";
import { useTranslation } from "@/shared/i18n";

/**
 * The first launch: no question. One way forward, starting Ahab
 * here; a quiet way to an Ahab server that runs elsewhere. Whether this
 * computer should also be a machine of that server is asked later, in Ahab.
 */
export function Welcome({ problem }: { problem?: string }) {
	const { t } = useTranslation("native");
	const phone = isPhoneApp();
	const [computer, setComputer] = useState<ThisComputer>();
	useEffect(() => {
		void native.thisComputer().then(setComputer, () => setComputer(undefined));
	}, []);
	const why = !computer
		? undefined
		: !(computer.installed || computer.bundled)
			? t("welcome.noAhab")
			: !computer.serviceManager
				? t("welcome.noServiceManager")
				: undefined;
	// A phone is a client only: it connects, it never runs Ahab.
	if (phone)
		return (
			<Hero title={t("welcome.title")} lead={t("welcome.phone.lead")}>
				{problem ? <Problem>{problem}</Problem> : null}
				<Button size="lg" className="h-12 w-full max-w-xs text-base" onClick={() => go("/connect")}>
					{t("welcome.phone.connect")}
				</Button>
			</Hero>
		);
	return (
		<Hero title={t("welcome.title")} lead={t("welcome.lead")}>
			{problem ? <Problem>{problem}</Problem> : null}
			{why ? (
				<>
					<p className="text-muted-foreground text-sm text-pretty">{why}</p>
					<Button
						size="lg"
						className="h-12 w-full max-w-xs text-base"
						onClick={() => go("/connect")}
					>
						{t("welcome.connect")}
					</Button>
				</>
			) : (
				<>
					<Button
						size="lg"
						className="h-12 w-full max-w-xs text-base"
						disabled={!computer}
						onClick={() => go("/running", { action: "run" })}
					>
						{t("screens.startHere")}
					</Button>
					<button
						type="button"
						onClick={() => go("/connect")}
						className="text-muted-foreground hover:text-foreground focus-visible:ring-ring/50 rounded text-sm underline-offset-4 transition-colors outline-none hover:underline focus-visible:ring-[3px]"
					>
						{t("welcome.haveServer")}
					</button>
				</>
			)}
		</Hero>
	);
}
