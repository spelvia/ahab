import type {
	AgentState,
	AgentStatus,
	AgentUpdate,
	AgentUsage,
	UsageWindow,
} from "@/shared/api/generated/ahab.schemas";
import { t } from "@/shared/i18n";

/** The agents Ahab knows, by their stable identifiers, with their names. */
const AGENTS: Record<string, string> = {
	"claude-code": "Claude Code",
	codex: "Codex",
	pi: "Pi",
};

/**
 * An agent's readable name: Ahab's own by name; an added one (`acp:qwen-code`,
 * `custom:my-agent`) from its identifier ("Qwen Code"). Lists that know the
 * agent's own name (detection's label) show that instead.
 */
export function agentLabel(agent: string): string {
	const known = AGENTS[agent];
	if (known) return known;
	const name = agent.replace(/^(acp|custom):/, "");
	return name
		.split(/[-_.]/)
		.filter(Boolean)
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
		.join(" ");
}

/** What a person runs on the machine to sign an agent in, when Ahab knows it. */
const SIGN_IN: Record<string, string> = {
	"claude-code": "claude auth login",
	codex: "codex login",
};

export function signInCommand(agent: string): string | undefined {
	return SIGN_IN[agent];
}

/** An agent's state as a word, and the pill it shows in. */
export function stateOf(state: AgentState): { word: string; pill: string } {
	const word = t(`agentState.${state}`, { ns: "machines" });
	switch (state) {
		case "ready":
			return { word, pill: "pill-ok" };
		case "notSignedIn":
		case "unsupportedVersion":
			return { word, pill: "pill-warn" };
		case "notInstalled":
			return { word, pill: "pill-muted" };
		case "error":
			return { word, pill: "pill-danger" };
	}
}

/** One agent on a machine with all Ahab knows about it there. */
export interface AgentView {
	status: AgentStatus;
	update?: AgentUpdate;
	usage?: AgentUsage;
}

/** The list's filters. */
export type Facet = "all" | "attention" | "ready" | "update" | "installed" | "missing";

/** The filters in their order; their labels are `facets.<id>`. */
export const FACETS: readonly Facet[] = [
	"all",
	"attention",
	"ready",
	"update",
	"installed",
	"missing",
];

export function installed(view: AgentView): boolean {
	return view.status.state !== "notInstalled";
}

/** The plan limit closest to running out, if any. */
export function tightest(view: AgentView): UsageWindow | undefined {
	if (view.usage?.status !== "available") return undefined;
	return view.usage.windows.reduce<UsageWindow | undefined>(
		(most, window) => (!most || window.usedPercent > most.usedPercent ? window : most),
		undefined,
	);
}

/** Whether something about the agent needs a person: a sign-in, a problem, a limit nearly used up. */
function needsYou(view: AgentView): boolean {
	const state = view.status.state;
	if (state === "notSignedIn" || state === "error" || state === "unsupportedVersion") return true;
	return (tightest(view)?.usedPercent ?? 0) >= 90;
}

export function updatable(view: AgentView): boolean {
	return Boolean(view.update?.available);
}

export function inFacet(view: AgentView, facet: Facet): boolean {
	switch (facet) {
		case "all":
			return true;
		case "attention":
			return needsYou(view);
		case "ready":
			return view.status.state === "ready";
		case "update":
			return updatable(view);
		case "installed":
			return installed(view);
		case "missing":
			return !installed(view);
	}
}

/** Whether `query` names the agent: its name, identifier or account. */
export function matches(view: AgentView, query: string): boolean {
	const wanted = query.trim().toLowerCase();
	if (!wanted) return true;
	return [view.status.label, view.status.agent, view.status.account ?? "", view.usage?.plan ?? ""]
		.join(" ")
		.toLowerCase()
		.includes(wanted);
}

/** The list's sections: what needs you first, then the rest installed, then the others. */
export function sections(views: readonly AgentView[]): {
	attention: AgentView[];
	installed: AgentView[];
	missing: AgentView[];
} {
	const byName = (a: AgentView, b: AgentView) => a.status.label.localeCompare(b.status.label);
	return {
		attention: views.filter((view) => installed(view) && needsYou(view)).toSorted(byName),
		installed: views.filter((view) => installed(view) && !needsYou(view)).toSorted(byName),
		missing: views.filter((view) => !installed(view)).toSorted(byName),
	};
}
