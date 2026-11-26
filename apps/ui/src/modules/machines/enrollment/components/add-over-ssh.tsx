import { ExternalLink, X } from "lucide-react";

import { docs } from "@/shared/docs";
import { useTranslation } from "@/shared/i18n";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "@/shared/ui/copy-button";

/** The command that adds a computer over SSH. */
const SSH_COMMAND = "ahab machines add you@host";

/**
 * Adding a machine over SSH, inline under the page's header: the command to
 * run where the Ahab CLI reaches this server. It installs Ahab on the other
 * computer, joins it and starts it, with SSH as the proof.
 */
export function AddOverSshPanel({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation("machines");
	const { t: common } = useTranslation();
	return (
		<section
			aria-labelledby="add-over-ssh-heading"
			className="border-border bg-card rounded-lg border px-4 py-4"
		>
			<header className="flex items-start justify-between gap-4">
				<div>
					<h2 id="add-over-ssh-heading" className="text-base font-bold">
						{t("addOverSsh.title")}
					</h2>
					<p className="text-muted-foreground mt-1 text-sm">{t("addOverSsh.intro")}</p>
				</div>
				<Button
					variant="ghost"
					size="icon-sm"
					aria-label={common("actions.close")}
					onClick={onClose}
				>
					<X />
				</Button>
			</header>

			<div className="bg-background border-border mt-3 flex items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-3">
				<code className="text-heading min-h-5 flex-1 font-mono text-sm break-all">
					{SSH_COMMAND}
				</code>
				<CopyButton text={SSH_COMMAND} label={t("addOverSsh.copyLabel")} />
			</div>

			<p className="text-muted-foreground mt-3 text-sm text-pretty">
				{t("addOverSsh.body")}{" "}
				<a
					className="text-heading inline-flex items-center gap-1 underline underline-offset-2"
					href={docs("/installation/cli/#add-a-machine-over-ssh")}
					target="_blank"
					rel="noreferrer"
				>
					{t("addOverSsh.docs")}
					<ExternalLink className="size-3" aria-hidden />
				</a>
			</p>
		</section>
	);
}
