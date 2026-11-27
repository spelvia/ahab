import { describe, expect, it } from "vitest";

import { agentLabel } from "@/modules/machines/agents/model";

describe("agents", () => {
	it("names the agents Ahab knows, and makes added ones readable", () => {
		expect(agentLabel("claude-code")).toBe("Claude Code");
		expect(agentLabel("acp:qwen-code")).toBe("Qwen Code");
		expect(agentLabel("custom:my_agent")).toBe("My Agent");
	});
});
