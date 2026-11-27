import { cn } from "cn";
import { Bot } from "lucide-react";

import { useAgentCatalog } from "@/modules/machines/agents/api";

/**
 * An agent's logo, as Stubb's profile of it says (Claude's mark for Claude
 * Code, OpenAI's for Codex, the registry's for ACP agents), served by Ahab:
 * one-colour logos in the text's colour, so they read on light and dark;
 * others in their own. A neutral robot for agents without one (custom
 * agents). Decorative: the agent's name is always written next to it.
 */
export function AgentIcon({ agent, className }: { agent: string; className?: string }) {
	const catalog = useAgentCatalog();
	const entry = catalog.data?.items.find((item) => item.id === agent);
	if (!entry?.hasLogo) return <Bot aria-hidden className={cn("size-4 shrink-0", className)} />;
	const url = `/api/v1/agents/${encodeURIComponent(agent)}/logo`;
	if (entry.monochrome)
		return (
			<span
				aria-hidden
				className={cn("inline-block size-4 shrink-0 bg-current", className)}
				style={{
					maskImage: `url("${url}")`,
					maskSize: "contain",
					maskRepeat: "no-repeat",
					maskPosition: "center",
				}}
			/>
		);
	return (
		<img
			src={url}
			alt=""
			aria-hidden
			draggable={false}
			className={cn("size-4 shrink-0", className)}
		/>
	);
}
