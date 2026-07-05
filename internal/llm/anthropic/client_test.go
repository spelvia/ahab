package anthropic

import (
	"encoding/json"
	"testing"

	sdk "github.com/anthropics/anthropic-sdk-go"

	"github.com/spelvia/ahab/internal/llm"
)

func TestToSDKMessagesRoundTripShape(t *testing.T) {
	msgs := []llm.Message{
		llm.TextMessage(llm.RoleUser, "deploy nginx"),
		{Role: llm.RoleAssistant, Blocks: []llm.Block{
			{Type: llm.BlockThinking, Text: "planning", Signature: "sig123"},
			{Type: llm.BlockText, Text: "I'll check the cluster."},
			{Type: llm.BlockToolUse, ToolID: "tu_1", ToolName: "cluster_read", Input: json.RawMessage(`{"args":["get","pods"]}`)},
		}},
		{Role: llm.RoleUser, Blocks: []llm.Block{
			{Type: llm.BlockToolResult, ToolID: "tu_1", Content: "no pods", IsError: false},
		}},
	}

	out := toSDKMessages(msgs)
	if len(out) != 3 {
		t.Fatalf("got %d messages, want 3", len(out))
	}
	if out[0].Role != sdk.MessageParamRoleUser || out[1].Role != sdk.MessageParamRoleAssistant {
		t.Fatalf("unexpected roles: %v, %v", out[0].Role, out[1].Role)
	}
	if len(out[1].Content) != 3 {
		t.Fatalf("assistant message has %d blocks, want 3", len(out[1].Content))
	}
	tu := out[1].Content[2].OfToolUse
	if tu == nil || tu.ID != "tu_1" || tu.Name != "cluster_read" {
		t.Fatalf("tool_use block not converted: %+v", out[1].Content[2])
	}
	// Input must marshal as raw JSON, not base64.
	data, err := json.Marshal(tu.Input)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != `{"args":["get","pods"]}` {
		t.Fatalf("tool input marshaled as %s", data)
	}
	tr := out[2].Content[0].OfToolResult
	if tr == nil || tr.ToolUseID != "tu_1" {
		t.Fatalf("tool_result block not converted: %+v", out[2].Content[0])
	}
}

func TestToSDKTools(t *testing.T) {
	tools := toSDKTools([]llm.ToolDef{{
		Name:        "read_file",
		Description: "Read a file",
		InputSchema: map[string]any{"path": map[string]any{"type": "string"}},
		Required:    []string{"path"},
	}})
	if len(tools) != 1 || tools[0].OfTool == nil {
		t.Fatalf("unexpected tools: %+v", tools)
	}
	if tools[0].OfTool.Name != "read_file" {
		t.Fatalf("name = %q", tools[0].OfTool.Name)
	}
}

func TestFromSDKStopReason(t *testing.T) {
	cases := map[sdk.StopReason]llm.StopReason{
		sdk.StopReasonEndTurn:   llm.StopEndTurn,
		sdk.StopReasonToolUse:   llm.StopToolUse,
		sdk.StopReasonMaxTokens: llm.StopMaxTokens,
		sdk.StopReasonRefusal:   llm.StopRefusal,
		sdk.StopReason("weird"): llm.StopOther,
	}
	for in, want := range cases {
		if got := fromSDKStopReason(in); got != want {
			t.Errorf("fromSDKStopReason(%q) = %q, want %q", in, got, want)
		}
	}
}
