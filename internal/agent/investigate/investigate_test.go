package investigate

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"

	"github.com/spelvia/ahab/internal/executor"
	"github.com/spelvia/ahab/internal/llm/llmtest"
)

func deps(t *testing.T, fake *llmtest.Fake) Deps {
	t.Helper()
	return Deps{
		Provider:   fake,
		Runner:     executor.New("", ""),
		WorkDir:    t.TempDir(),
		ReportsDir: t.TempDir(),
		SessionID:  "test-session",
		MaxTokens:  100,
	}
}

func TestRunSavesReport(t *testing.T) {
	report, _ := json.Marshal(map[string]string{
		"title":  "Bad Image Tag",
		"report": "## Symptom\nImagePullBackOff\n\n## Root cause\nTag v9.9.9 does not exist.",
	})
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "submit_report", string(report)),
		llmtest.TextTurn("Report submitted."),
	}}
	d := deps(t, fake)

	path, err := Run(context.Background(), d, "payments pod is ImagePullBackOff")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(path, "test-session-bad-image-tag.md") {
		t.Fatalf("unexpected report path %q", path)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "Tag v9.9.9 does not exist") {
		t.Fatalf("report content missing: %s", data)
	}

	// The system prompt must mention the missing-repos situation.
	if !strings.Contains(fake.Requests[0].System, "No source repositories are linked") {
		t.Error("system prompt missing repo note")
	}
}

func TestRunFailsWithoutReport(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.TextTurn("I looked around but forgot to file a report."),
	}}
	if _, err := Run(context.Background(), deps(t, fake), "something is wrong"); err == nil {
		t.Fatal("expected error when no report was submitted")
	}
}
