package tools

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm"
)

type commandInput struct {
	Executor string   `json:"executor"`
	Args     []string `json:"args"`
}

func (c *commandInput) normalize() {
	if c.Executor == "" {
		c.Executor = "kubectl"
	}
}

func (c commandInput) command() executor.Command {
	return executor.Command{Executor: c.Executor, Args: c.Args}
}

var commandSchema = map[string]any{
	"executor": map[string]any{
		"type":        "string",
		"enum":        executor.Executors(),
		"description": "Which CLI to run (default kubectl)",
	},
	"args": map[string]any{
		"type":        "array",
		"items":       map[string]any{"type": "string"},
		"description": "Arguments after the binary name, one array element per argument (no shell quoting)",
	},
}

func describeCommand(r *executor.Runner, raw json.RawMessage) (commandInput, string) {
	var in commandInput
	_ = Input(raw, &in)
	in.normalize()
	return in, strings.Join(r.CommandLine(in.command()), " ")
}

// ClusterRead returns the auto-approved, read-only cluster inspection tool.
func ClusterRead(r *executor.Runner) *Tool {
	return &Tool{
		Def: llm.ToolDef{
			Name:        "cluster_read",
			Description: "Inspect cluster state with read-only commands (kubectl get/describe/logs/events/top, helm list/status, flux get, argocd app list/get). Mutating verbs are rejected.",
			InputSchema: commandSchema,
			Required:    []string{"args"},
		},
		Policy: PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			_, line := describeCommand(r, raw)
			return line
		},
		Run: func(ctx context.Context, raw json.RawMessage) (string, error) {
			var in commandInput
			if err := Input(raw, &in); err != nil {
				return "", err
			}
			in.normalize()
			cmd := in.command()
			if err := r.Validate(cmd); err != nil {
				return "", err
			}
			if !r.IsRead(cmd) {
				return "", errors.New("cluster_read only accepts read-only commands; use run_command for mutations")
			}
			return runResult(r, ctx, cmd)
		},
	}
}

// PreviewCommand returns the auto-approved dry-run preview tool.
func PreviewCommand(r *executor.Runner) *Tool {
	return &Tool{
		Def: llm.ToolDef{
			Name:        "preview_command",
			Description: "Preview a mutating command without changing the cluster (kubectl --dry-run=server, helm --dry-run, flux --export, argocd app sync --dry-run). Use this before requesting run_command approval.",
			InputSchema: commandSchema,
			Required:    []string{"args"},
		},
		Policy: PolicyAuto,
		Phase:  "apply",
		Describe: func(raw json.RawMessage) string {
			_, line := describeCommand(r, raw)
			return "preview: " + line
		},
		Run: func(ctx context.Context, raw json.RawMessage) (string, error) {
			var in commandInput
			if err := Input(raw, &in); err != nil {
				return "", err
			}
			in.normalize()
			out, err := r.Preview(ctx, in.command())
			if errors.Is(err, executor.ErrNoPreview) {
				return "", err
			}
			return out, err
		},
	}
}

// RunCommand returns the gated mutating-command tool. Every invocation
// requires interactive approval.
func RunCommand(r *executor.Runner) *Tool {
	return &Tool{
		Def: llm.ToolDef{
			Name:        "run_command",
			Description: "Run an allowlisted kubectl / helm / flux / argocd command that changes cluster state. The user reviews the exact command before it executes; include your reasoning in the surrounding text.",
			InputSchema: commandSchema,
			Required:    []string{"args"},
		},
		Policy: PolicyAsk,
		Phase:  "apply",
		Describe: func(raw json.RawMessage) string {
			_, line := describeCommand(r, raw)
			return line
		},
		Detail: func(raw json.RawMessage) string {
			_, line := describeCommand(r, raw)
			return line
		},
		Run: func(ctx context.Context, raw json.RawMessage) (string, error) {
			var in commandInput
			if err := Input(raw, &in); err != nil {
				return "", err
			}
			in.normalize()
			return runResult(r, ctx, in.command())
		},
	}
}

func runResult(r *executor.Runner, ctx context.Context, cmd executor.Command) (string, error) {
	res, err := r.Run(ctx, cmd)
	if err != nil {
		return "", err
	}
	if res.ExitCode != 0 {
		return "", fmt.Errorf("exit code %d\n%s", res.ExitCode, res.Output)
	}
	if strings.TrimSpace(res.Output) == "" {
		return "(command succeeded with no output)", nil
	}
	return res.Output, nil
}
