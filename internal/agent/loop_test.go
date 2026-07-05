package agent

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/spelvia/ahab/internal/agent/tools"
	"github.com/spelvia/ahab/internal/llm"
	"github.com/spelvia/ahab/internal/llm/llmtest"
)

func echoTool(policy tools.Policy) *tools.Tool {
	return &tools.Tool{
		Def: llm.ToolDef{
			Name:        "echo",
			Description: "Echo the message back",
			InputSchema: map[string]any{"message": map[string]any{"type": "string"}},
			Required:    []string{"message"},
		},
		Policy: policy,
		Describe: func(input json.RawMessage) string {
			return "echo a message"
		},
		Run: func(_ context.Context, input json.RawMessage) (string, error) {
			var in struct {
				Message string `json:"message"`
			}
			if err := tools.Input(input, &in); err != nil {
				return "", err
			}
			return "echo: " + in.Message, nil
		},
	}
}

type memRecorder struct{ records []Record }

func (m *memRecorder) Record(r Record) { m.records = append(m.records, r) }

func TestLoopExecutesAutoToolAndFinishes(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "echo", `{"message":"ahoy"}`),
		llmtest.TextTurn("done"),
	}}
	rec := &memRecorder{}
	l := &Loop{
		Provider:  fake,
		Registry:  tools.NewRegistry(echoTool(tools.PolicyAuto)),
		Recorder:  rec,
		MaxTokens: 100,
	}
	if err := l.Run(context.Background(), "say ahoy"); err != nil {
		t.Fatal(err)
	}

	// Second request must carry the tool result back to the model.
	if len(fake.Requests) != 2 {
		t.Fatalf("got %d requests, want 2", len(fake.Requests))
	}
	last := fake.Requests[1].Messages
	final := last[len(last)-1]
	if final.Role != llm.RoleUser || final.Blocks[0].Type != llm.BlockToolResult {
		t.Fatalf("final message is not a tool_result: %+v", final)
	}
	if final.Blocks[0].Content != "echo: ahoy" || final.Blocks[0].IsError {
		t.Fatalf("unexpected tool result: %+v", final.Blocks[0])
	}
	if len(rec.records) != 1 || rec.records[0].Approval != "auto" {
		t.Fatalf("unexpected records: %+v", rec.records)
	}
}

func TestLoopDenialFeedbackReachesModel(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "echo", `{"message":"rm -rf"}`),
		llmtest.TextTurn("understood, adjusting"),
	}}
	denied := GateFunc(func(_ context.Context, req ApprovalRequest) (ApprovalResponse, error) {
		return ApprovalResponse{Approved: false, Feedback: "use --dry-run first"}, nil
	})
	rec := &memRecorder{}
	l := &Loop{
		Provider:  fake,
		Registry:  tools.NewRegistry(echoTool(tools.PolicyAsk)),
		Gate:      denied,
		Recorder:  rec,
		MaxTokens: 100,
	}
	if err := l.Run(context.Background(), "wipe it"); err != nil {
		t.Fatal(err)
	}

	last := fake.Requests[1].Messages
	result := last[len(last)-1].Blocks[0]
	if !result.IsError || !strings.Contains(result.Content, "use --dry-run first") {
		t.Fatalf("denial feedback missing from tool result: %+v", result)
	}
	if rec.records[0].Approval != "denied" {
		t.Fatalf("record approval = %q, want denied", rec.records[0].Approval)
	}
}

func TestLoopUnknownTool(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{
		llmtest.ToolTurn("tu_1", "no_such_tool", `{}`),
		llmtest.TextTurn("ok"),
	}}
	l := &Loop{Provider: fake, Registry: tools.NewRegistry(), MaxTokens: 100}
	if err := l.Run(context.Background(), "hi"); err != nil {
		t.Fatal(err)
	}
	result := fake.Requests[1].Messages
	block := result[len(result)-1].Blocks[0]
	if !block.IsError || !strings.Contains(block.Content, "unknown tool") {
		t.Fatalf("expected unknown-tool error, got %+v", block)
	}
}

func TestLoopRefusal(t *testing.T) {
	fake := &llmtest.Fake{Turns: []llmtest.Turn{{
		Message: llm.TextMessage(llm.RoleAssistant, ""),
		Stop:    llm.StopRefusal,
	}}}
	l := &Loop{Provider: fake, Registry: tools.NewRegistry(), MaxTokens: 100}
	if err := l.Run(context.Background(), "hi"); err != ErrRefused {
		t.Fatalf("err = %v, want ErrRefused", err)
	}
}

func TestTruncate(t *testing.T) {
	long := strings.Repeat("x", maxToolResultBytes*2)
	got := truncate(long, maxToolResultBytes)
	if len(got) >= len(long) || !strings.Contains(got, "truncated") {
		t.Fatalf("truncate failed: len=%d", len(got))
	}
	if short := truncate("ok", maxToolResultBytes); short != "ok" {
		t.Fatalf("short string modified: %q", short)
	}
}
