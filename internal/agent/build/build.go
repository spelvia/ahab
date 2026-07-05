// Package build implements Building mode: a PLAN → WRITE → APPLY companion
// workflow in which the agent proposes a plan, writes manifests, and applies
// them — with the user approving the plan, every file diff, and every
// mutating command.
package build

import (
	"context"
	"encoding/json"
	"strings"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/agent/tools"
	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm"
)

const systemPrompt = `You are ahab, an infrastructure agent that builds Kubernetes systems under human supervision. Work in three phases, strictly in order:

PLAN
- Explore the current cluster state (cluster_read) and any existing manifests (list_files, read_file, grep) first.
- Call submit_plan with a concrete plan: what you will create or change, which files, which commands, and how you will verify success. The user must approve the plan before you touch anything.
- If the plan is denied, revise it using the feedback and submit again.

WRITE
- Create or modify manifests and Helm templates with write_file. The user reviews every diff.
- Keep files small and idiomatic: one logical resource group per file, standard labels, explicit namespaces.

APPLY
- Before each mutating command, run preview_command (server-side dry-run) and check the result.
- Then request the real command with run_command. Explain in one sentence what the command does and why, right before calling it.
- Apply changes in dependency order (namespaces/CRDs before workloads).
- After applying, monitor with cluster_read (rollout status, pod readiness, events) until the system is stable. If something is wrong, diagnose it, fix the manifests (back to WRITE), and re-apply.

Rules:
- Never skip plan approval. Never claim an apply succeeded without verifying it with cluster_read.
- If the user denies a command, adjust according to their feedback rather than retrying the same thing.
- Finish with a short summary: what was deployed, current status, and how to roll back.`

// Deps carries everything Building mode needs.
type Deps struct {
	Provider  llm.Provider
	Runner    *executor.Runner
	UI        agent.UI
	Recorder  agent.Recorder
	Gate      agent.Gate
	Ask       tools.AskFunc
	WorkDir   string
	Repos     []string
	MaxTokens int
}

// Run executes one supervised build session.
func Run(ctx context.Context, d Deps, prompt string) error {
	loop, err := New(d)
	if err != nil {
		return err
	}
	return loop.Run(ctx, prompt)
}

// New assembles the Building-mode agent loop. Callers that support follow-up
// turns (the TUI) keep the loop and call Run on it repeatedly.
func New(d Deps) (*agent.Loop, error) {
	readRoots, err := tools.NewRoots(d.WorkDir, d.Repos...)
	if err != nil {
		return nil, err
	}
	// Writes are confined to the project directory only.
	writeRoots, err := tools.NewRoots(d.WorkDir)
	if err != nil {
		return nil, err
	}

	registry := tools.NewRegistry(
		tools.ClusterRead(d.Runner),
		tools.PreviewCommand(d.Runner),
		tools.RunCommand(d.Runner),
		tools.ReadFile(readRoots),
		tools.ListFiles(readRoots),
		tools.Grep(readRoots),
		tools.WriteFile(writeRoots),
		submitPlan(),
	)
	if d.Ask != nil {
		registry.Add(tools.AskUser(d.Ask))
	}

	return &agent.Loop{
		Provider:  d.Provider,
		Registry:  registry,
		Gate:      d.Gate,
		UI:        d.UI,
		Recorder:  d.Recorder,
		System:    systemPrompt,
		MaxTokens: d.MaxTokens,
		Phase:     "plan",
	}, nil
}

// submitPlan returns the gated plan-approval tool. Approval advances the
// session; denial feedback sends the agent back to revise.
func submitPlan() *tools.Tool {
	type in struct {
		Title string `json:"title"`
		Plan  string `json:"plan"`
	}
	return &tools.Tool{
		Def: llm.ToolDef{
			Name:        "submit_plan",
			Description: "Submit the infrastructure change plan for user approval. Required before writing files or running commands.",
			InputSchema: map[string]any{
				"title": map[string]any{"type": "string", "description": "One-line plan title"},
				"plan":  map[string]any{"type": "string", "description": "Markdown plan: changes, files, commands, verification steps"},
			},
			Required: []string{"title", "plan"},
		},
		Policy: tools.PolicyAsk,
		Phase:  "plan",
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = tools.Input(raw, &i)
			return "plan: " + i.Title
		},
		Detail: func(raw json.RawMessage) string {
			var i in
			_ = tools.Input(raw, &i)
			return strings.TrimSpace(i.Plan)
		},
		Run: func(context.Context, json.RawMessage) (string, error) {
			return "Plan approved by the user. Proceed to the WRITE phase.", nil
		},
	}
}
