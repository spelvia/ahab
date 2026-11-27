/**
 * The agents on the landing page: the three Ahab drives itself, then every
 * agent in Stubb's bundled ACP registry, the best known first (a judgement,
 * not a count). `logo` names the agent's icon from the ACP registry, in
 * src/assets/landing/agents/ (see its README); without one, the page shows
 * a lettermark.
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
	{ name: "Claude Code", logo: "claude-acp", hex: "#D97757" },
	{ name: "Codex", logo: "codex-acp" },
	{ name: "Pi", logo: "pi-acp" },
];

export const ACP: Agent[] = [
	{ name: "Gemini CLI", logo: "gemini", hex: "#8E75B2" },
	{ name: "GitHub Copilot", logo: "github-copilot-cli" },
	{ name: "Cursor", logo: "cursor" },
	{ name: "OpenCode", logo: "opencode" },
	{ name: "Google Antigravity", logo: "antigravity-acp" },
	{ name: "Cline", logo: "cline" },
	{ name: "Qwen Code", logo: "qwen-code", hex: "#6950EF" },
	{ name: "goose", logo: "goose" },
	{ name: "Kiro CLI", logo: "kiro" },
	{ name: "Mistral Vibe", logo: "mistral-vibe", hex: "#FA520F" },
	{ name: "Kimi Code", logo: "kimi-code" },
	{ name: "Junie", logo: "junie" },
	{ name: "Devin", logo: "devin" },
	{ name: "Factory Droid", logo: "factory-droid" },
	{ name: "Amp", logo: "amp-acp" },
	{ name: "Auggie CLI", logo: "auggie" },
	{ name: "Kilo", logo: "kilo" },
	{ name: "Grok Build", logo: "grok-build" },
	{ name: "Claude Agent", logo: "claude-acp", hex: "#D97757" },
	{ name: "Codex", logo: "codex-acp" },
	{ name: "Cortex Code", logo: "cortex-code", hex: "#29B5E8" },
	{ name: "OpenCode V2", logo: "opencode" },
	{ name: "Kimi CLI (Legacy)", logo: "kimi" },
	{ name: "MiniMax Code", logo: "minimax-code", hex: "#E73562" },
	{ name: "DeepAgents", logo: "deepagents", hex: "#7FC8FF" },
	{ name: "Muse Code", logo: "muse-code", hex: "#0467DF" },
	{ name: "Bob", logo: "bob" },
	{ name: "GLM Agent", logo: "glm-acp-agent" },
	{ name: "Codebuddy Code", logo: "codebuddy-code" },
	{ name: "Qoder CLI", logo: "qoder" },
	{ name: "fast-agent", logo: "fast-agent" },
	{ name: "Poolside", logo: "poolside" },
	{ name: "VT Code", logo: "vtcode" },
	{ name: "Stakpak", logo: "stakpak" },
	{ name: "pi ACP", logo: "pi-acp" },
	{ name: "Autohand Code", logo: "autohand" },
	{ name: "Corust Agent", logo: "corust-agent" },
	{ name: "crow-cli", logo: "crow-cli" },
	{ name: "Dirac", logo: "dirac" },
	{ name: "DimCode", logo: "dimcode" },
	{ name: "Harn", logo: "harn" },
	{ name: "Kimchi", logo: "kimchi" },
	{ name: "Minion Code", logo: "minion-code" },
	{ name: "Nova", logo: "nova" },
	{ name: "siGit Code", logo: "sigit" },
	{ name: "Agoragentic", logo: "agoragentic-acp" },
];
