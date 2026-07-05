package build

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm/llmtest"
)

type memRecorder struct{ records []agent.Record }

func (m *memRecorder) Record(r agent.Record) { m.records = append(m.records, r) }

func mustJSON(t *testing.T, v any) string {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// approveAll approves everything and remembers what it was asked.
type approveAll struct{ requests []agent.ApprovalRequest }

func (a *approveAll) Confirm(_ context.Context, req agent.ApprovalRequest) (agent.ApprovalResponse, error) {
	a.requests = append(a.requests, req)
	return agent.ApprovalResponse{Approved: true}, nil
}

func TestBuildPlanThenWriteFlow(t *testing.T) {
	work := t.TempDir()
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "submit_plan", mustJSON(t, map[string]string{
			"title": "Deploy nginx",
			"plan":  "1. Write deploy.yaml\n2. kubectl apply",
		})),
		llmtest.ToolTurn("tu_2", "write_file", mustJSON(t, map[string]string{
			"path":    "manifests/deploy.yaml",
			"content": "kind: Deployment\nmetadata:\n  name: nginx\n",
		})),
		llmtest.TextTurn("Manifests written; ready to apply when a cluster is available."),
	}}
	gate := &approveAll{}
	rec := &memRecorder{}

	err := Run(context.Background(), Deps{
		Provider:  fake,
		Runner:    executor.New("", ""),
		Gate:      gate,
		Recorder:  rec,
		WorkDir:   work,
		MaxTokens: 100,
	}, "deploy nginx")
	if err != nil {
		t.Fatal(err)
	}

	// Both gated calls asked for approval, with reviewable detail.
	if len(gate.requests) != 2 {
		t.Fatalf("got %d approval requests, want 2: %+v", len(gate.requests), gate.requests)
	}
	if gate.requests[0].Tool != "submit_plan" || !strings.Contains(gate.requests[0].Detail, "Write deploy.yaml") {
		t.Fatalf("plan approval request wrong: %+v", gate.requests[0])
	}
	if gate.requests[1].Tool != "write_file" || !strings.Contains(gate.requests[1].Detail, "+ kind: Deployment") {
		t.Fatalf("write approval should show a diff: %+v", gate.requests[1])
	}

	// The file landed inside the workdir.
	data, err := os.ReadFile(filepath.Join(work, "manifests", "deploy.yaml"))
	if err != nil || !strings.Contains(string(data), "name: nginx") {
		t.Fatalf("file not written: %v", err)
	}

	// Records carry the phase transition plan → write.
	if rec.records[0].Phase != "plan" || rec.records[1].Phase != "write" {
		t.Fatalf("phases = %q, %q", rec.records[0].Phase, rec.records[1].Phase)
	}
}

func TestBuildPlanDenialFeedback(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "submit_plan", mustJSON(t, map[string]string{
			"title": "Delete everything", "plan": "wipe the namespace",
		})),
		llmtest.TextTurn("Understood, I'll rework the plan."),
	}}
	gate := agent.GateFunc(func(_ context.Context, req agent.ApprovalRequest) (agent.ApprovalResponse, error) {
		return agent.ApprovalResponse{Approved: false, Feedback: "too destructive, scope it to the app"}, nil
	})

	err := Run(context.Background(), Deps{
		Provider:  fake,
		Runner:    executor.New("", ""),
		Gate:      gate,
		WorkDir:   t.TempDir(),
		MaxTokens: 100,
	}, "clean up")
	if err != nil {
		t.Fatal(err)
	}
	last := fake.Requests[1].Messages
	result := last[len(last)-1].Blocks[0]
	if !result.IsError || !strings.Contains(result.Content, "too destructive") {
		t.Fatalf("denial feedback missing: %+v", result)
	}
}

func TestBuildWriteOutsideWorkdirRejected(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "write_file", mustJSON(t, map[string]string{
			"path": "../outside.yaml", "content": "kind: Evil",
		})),
		llmtest.TextTurn("ok"),
	}}
	gate := &approveAll{}
	work := t.TempDir()

	err := Run(context.Background(), Deps{
		Provider:  fake,
		Runner:    executor.New("", ""),
		Gate:      gate,
		WorkDir:   work,
		MaxTokens: 100,
	}, "write somewhere weird")
	if err != nil {
		t.Fatal(err)
	}
	if _, statErr := os.Stat(filepath.Join(filepath.Dir(work), "outside.yaml")); statErr == nil {
		t.Fatal("file escaped the working directory")
	}
	last := fake.Requests[1].Messages
	result := last[len(last)-1].Blocks[0]
	if !result.IsError || !strings.Contains(result.Content, "outside the allowed directories") {
		t.Fatalf("escape not rejected: %+v", result)
	}
}
