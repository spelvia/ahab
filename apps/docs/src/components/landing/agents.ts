/**
 * The agents on the landing page: the three Ahab drives itself, then every
 * agent in Stubb's bundled ACP registry, the best known first (a judgement,
 * not a count). `logo` names an SVG in src/assets/landing/agents/ (see its
 * README for sources); without one, the page shows a lettermark.
 */
export interface Agent {
	name: string;
	logo?: string;
	/** The logo's own colour, shown on hover. */
	hex?: string;
	/** Through ACP: an adapter, or the agent's own ACP mode. */
	acp?: boolean;
}

export const NATIVE: Agent[] = [
	{ name: "Claude Code", logo: "claude", hex: "#D97757" },
	{ name: "Codex" },
	{ name: "Pi", logo: "pi" },
];

export const ACP: Agent[] = [
	{ name: "Gemini CLI", logo: "googlegemini", hex: "#8E75B2" },
	{ name: "GitHub Copilot", logo: "githubcopilot" },
	{ name: "Cursor", logo: "cursor" },
	{ name: "OpenCode", logo: "opencode" },
	{ name: "Google Antigravity" },
	{ name: "Cline", logo: "cline" },
	{ name: "Qwen Code", logo: "qwen", hex: "#6950EF" },
	{ name: "goose" },
	{ name: "Kiro CLI" },
	{ name: "Mistral Vibe", logo: "mistralai", hex: "#FA520F" },
	{ name: "Kimi Code", logo: "kimi" },
	{ name: "Junie", logo: "jetbrains" },
	{ name: "Devin" },
	{ name: "Factory Droid" },
	{ name: "Amp" },
	{ name: "Auggie CLI" },
	{ name: "Kilo" },
	{ name: "Grok Build" },
	{ name: "Claude Agent", logo: "claude", hex: "#D97757" },
	{ name: "Codex" },
	{ name: "Cortex Code", logo: "snowflake", hex: "#29B5E8" },
	{ name: "OpenCode V2", logo: "opencode" },
	{ name: "Kimi CLI (Legacy)", logo: "kimi" },
	{ name: "MiniMax Code", logo: "minimax", hex: "#E73562" },
	{ name: "DeepAgents", logo: "langchain", hex: "#7FC8FF" },
	{ name: "Muse Code", logo: "meta", hex: "#0467DF" },
	{ name: "Bob" },
	{ name: "GLM Agent" },
	{ name: "Codebuddy Code" },
	{ name: "Qoder CLI" },
	{ name: "fast-agent" },
	{ name: "Poolside" },
	{ name: "VT Code" },
	{ name: "Stakpak" },
	{ name: "pi ACP", logo: "pi" },
	{ name: "Autohand Code" },
	{ name: "Corust Agent" },
	{ name: "crow-cli" },
	{ name: "Dirac" },
	{ name: "DimCode" },
	{ name: "Harn" },
	{ name: "Kimchi" },
	{ name: "Minion Code" },
	{ name: "Nova" },
	{ name: "siGit Code" },
	{ name: "Agoragentic" },
];
