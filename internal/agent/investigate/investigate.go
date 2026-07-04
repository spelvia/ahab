// Package investigate implements Investigation mode: the agent traces a
// reported cluster symptom through cluster state, logs, and linked source
// repositories, then submits a root-cause report.
package investigate

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/agent/tools"
	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm"
)

const systemPrompt = `You are ahab, an SRE agent investigating a problem in a Kubernetes cluster.

Work methodically from symptom to root cause:
1. Reproduce the symptom with cluster_read (pod status, describe, events, logs, previous logs).
2. Follow the evidence: crash loops need logs and exit codes; scheduling failures need events and node capacity; config issues need the rendered resources.
3. If linked source repositories are available, trace the failing behavior into the code with list_files, grep, and read_file.
4. When you have established the root cause — or exhausted the available evidence — call submit_report exactly once with your findings.

Rules:
- You have read-only access: you cannot change the cluster. Never claim to have fixed anything.
- Ground every claim in evidence you actually observed in tool output; quote the relevant lines in the report.
- The report must contain: Symptom, Evidence, Root cause, Suggested fix (concrete commands or code changes the user could apply), and Confidence (high/medium/low with what would raise it).
- If evidence is inconclusive, say so and report the most likely candidates instead of guessing.`

// Deps carries everything Investigation mode needs.
type Deps struct {
	Provider   llm.Provider
	Runner     *executor.Runner
	UI         agent.UI
	Recorder   agent.Recorder
	Ask        tools.AskFunc
	WorkDir    string
	Repos      []string
	ReportsDir string
	SessionID  string
	MaxTokens  int
}

// Run executes one investigation and returns the saved report path.
func Run(ctx context.Context, d Deps, problem string) (string, error) {
	roots, err := tools.NewRoots(d.WorkDir, d.Repos...)
	if err != nil {
		return "", err
	}

	var reportPath string
	registry := tools.NewRegistry(
		tools.ClusterRead(d.Runner),
		tools.ReadFile(roots),
		tools.ListFiles(roots),
		tools.Grep(roots),
		submitReport(d.ReportsDir, d.SessionID, &reportPath),
	)
	if d.Ask != nil {
		registry.Add(tools.AskUser(d.Ask))
	}

	system := systemPrompt
	if len(d.Repos) > 0 {
		system += "\n\nLinked source repositories:\n- " + strings.Join(d.Repos, "\n- ")
	} else {
		system += "\n\nNo source repositories are linked; investigate from cluster state alone."
	}

	loop := &agent.Loop{
		Provider:  d.Provider,
		Registry:  registry,
		Gate:      agent.GateFunc(denyAll),
		UI:        d.UI,
		Recorder:  d.Recorder,
		System:    system,
		MaxTokens: d.MaxTokens,
		Phase:     "investigate",
	}
	if err := loop.Run(ctx, "Investigate this problem: "+problem); err != nil {
		return reportPath, err
	}
	if reportPath == "" {
		return "", fmt.Errorf("the agent finished without submitting a report")
	}
	return reportPath, nil
}

// denyAll backs the gate for a mode that registers no gated tools.
func denyAll(context.Context, agent.ApprovalRequest) (agent.ApprovalResponse, error) {
	return agent.ApprovalResponse{Approved: false, Feedback: "investigation mode is read-only"}, nil
}

var slugRe = regexp.MustCompile(`[^a-z0-9]+`)

func submitReport(dir, sessionID string, outPath *string) *tools.Tool {
	type in struct {
		Title  string `json:"title"`
		Report string `json:"report"`
	}
	return &tools.Tool{
		Def: llm.ToolDef{
			Name:        "submit_report",
			Description: "Submit the final investigation report (markdown). Call exactly once, when the investigation is complete.",
			InputSchema: map[string]any{
				"title":  map[string]any{"type": "string", "description": "Short report title"},
				"report": map[string]any{"type": "string", "description": "Full markdown report: Symptom, Evidence, Root cause, Suggested fix, Confidence"},
			},
			Required: []string{"title", "report"},
		},
		Policy: tools.PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = tools.Input(raw, &i)
			return "submit report: " + i.Title
		},
		Run: func(_ context.Context, raw json.RawMessage) (string, error) {
			var i in
			if err := tools.Input(raw, &i); err != nil {
				return "", err
			}
			if strings.TrimSpace(i.Report) == "" {
				return "", fmt.Errorf("report is empty")
			}
			slug := strings.Trim(slugRe.ReplaceAllString(strings.ToLower(i.Title), "-"), "-")
			if slug == "" {
				slug = "report"
			}
			path := filepath.Join(dir, fmt.Sprintf("%s-%s.md", sessionID, slug))
			content := fmt.Sprintf("# %s\n\n%s\n", i.Title, i.Report)
			if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
				return "", err
			}
			*outPath = path
			return "report saved to " + path, nil
		},
	}
}
