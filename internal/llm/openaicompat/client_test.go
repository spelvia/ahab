package openaicompat

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/spelvia/ahab/internal/llm"
)

// serve returns a test server that captures the request body and replays the
// given SSE lines, plus a Client pointed at it.
func serve(t *testing.T, sse []string, useMaxCompletion bool) (*Client, *json.RawMessage) {
	t.Helper()
	var captured json.RawMessage
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			t.Errorf("unexpected path %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-key" {
			t.Errorf("Authorization = %q", got)
		}
		body, err := json.RawMessage(nil), error(nil)
		if err = json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Errorf("decoding request: %v", err)
		}
		captured = body
		w.Header().Set("Content-Type", "text/event-stream")
		for _, line := range sse {
			_, _ = w.Write([]byte(line + "\n\n"))
		}
	}))
	t.Cleanup(srv.Close)
	return New(Config{
		BaseURL:                srv.URL + "/v1",
		APIKey:                 "test-key",
		Model:                  "test-model",
		UseMaxCompletionTokens: useMaxCompletion,
	}), &captured
}

func TestTextStream(t *testing.T) {
	client, _ := serve(t, []string{
		`data: {"choices":[{"delta":{"role":"assistant","content":""}}]}`,
		`data: {"choices":[{"delta":{"content":"Hello"}}]}`,
		`data: {"choices":[{"delta":{"content":" world"}}]}`,
		`data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
		`data: [DONE]`,
	}, false)

	var deltas []string
	msg, stop, err := client.Complete(context.Background(), llm.Request{
		Messages:  []llm.Message{llm.TextMessage(llm.RoleUser, "hi")},
		MaxTokens: 100,
	}, func(ev llm.Event) {
		if ev.Type == llm.EventTextDelta {
			deltas = append(deltas, ev.Text)
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	if stop != llm.StopEndTurn {
		t.Fatalf("stop = %q", stop)
	}
	if msg.Text() != "Hello world" {
		t.Fatalf("text = %q", msg.Text())
	}
	if strings.Join(deltas, "|") != "Hello| world" {
		t.Fatalf("deltas = %v", deltas)
	}
}

func TestToolCallStreamAssemblesArguments(t *testing.T) {
	client, _ := serve(t, []string{
		`data: {"choices":[{"delta":{"content":"Checking."}}]}`,
		`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"cluster_read","arguments":""}}]}}]}`,
		`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"args\":[\"get\""}}]}}]}`,
		`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":",\"pods\"]}"}}]}}]}`,
		`data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}`,
		`data: [DONE]`,
	}, false)

	var toolStarts []string
	msg, stop, err := client.Complete(context.Background(), llm.Request{
		Messages:  []llm.Message{llm.TextMessage(llm.RoleUser, "check pods")},
		MaxTokens: 100,
	}, func(ev llm.Event) {
		if ev.Type == llm.EventToolUseStart {
			toolStarts = append(toolStarts, ev.ToolName)
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	if stop != llm.StopToolUse {
		t.Fatalf("stop = %q", stop)
	}
	uses := msg.ToolUses()
	if len(uses) != 1 {
		t.Fatalf("got %d tool uses", len(uses))
	}
	if uses[0].ToolID != "call_abc" || uses[0].ToolName != "cluster_read" {
		t.Fatalf("tool use = %+v", uses[0])
	}
	var in struct {
		Args []string `json:"args"`
	}
	if err := json.Unmarshal(uses[0].Input, &in); err != nil {
		t.Fatalf("assembled arguments invalid: %v (%s)", err, uses[0].Input)
	}
	if len(in.Args) != 2 || in.Args[0] != "get" {
		t.Fatalf("args = %v", in.Args)
	}
	if len(toolStarts) != 1 || toolStarts[0] != "cluster_read" {
		t.Fatalf("toolStarts = %v", toolStarts)
	}
}

func TestToolCallsAuthoritativeOverStopFinish(t *testing.T) {
	// Some compat providers send finish_reason "stop" alongside tool calls.
	client, _ := serve(t, []string{
		`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"t","arguments":"{}"}}]}}]}`,
		`data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
		`data: [DONE]`,
	}, false)
	_, stop, err := client.Complete(context.Background(), llm.Request{
		Messages: []llm.Message{llm.TextMessage(llm.RoleUser, "x")}, MaxTokens: 10,
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if stop != llm.StopToolUse {
		t.Fatalf("stop = %q, want tool_use", stop)
	}
}

func TestReasoningContentBecomesThinking(t *testing.T) {
	client, _ := serve(t, []string{
		`data: {"choices":[{"delta":{"reasoning_content":"Let me think."}}]}`,
		`data: {"choices":[{"delta":{"content":"Answer."}}]}`,
		`data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
		`data: [DONE]`,
	}, false)

	var thinking []string
	msg, _, err := client.Complete(context.Background(), llm.Request{
		Messages: []llm.Message{llm.TextMessage(llm.RoleUser, "x")}, MaxTokens: 10,
	}, func(ev llm.Event) {
		if ev.Type == llm.EventThinkingDelta {
			thinking = append(thinking, ev.Text)
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	if msg.Blocks[0].Type != llm.BlockThinking || msg.Blocks[0].Text != "Let me think." {
		t.Fatalf("blocks[0] = %+v", msg.Blocks[0])
	}
	if len(thinking) != 1 {
		t.Fatalf("thinking deltas = %v", thinking)
	}
}

func TestRequestBodyShape(t *testing.T) {
	client, captured := serve(t, []string{
		`data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}`,
		`data: [DONE]`,
	}, false)

	history := []llm.Message{
		llm.TextMessage(llm.RoleUser, "check pods"),
		{Role: llm.RoleAssistant, Blocks: []llm.Block{
			{Type: llm.BlockThinking, Text: "reasoning that must not be sent back"},
			{Type: llm.BlockText, Text: "Checking."},
			{Type: llm.BlockToolUse, ToolID: "call_1", ToolName: "cluster_read", Input: []byte(`{"args":["get","pods"]}`)},
		}},
		{Role: llm.RoleUser, Blocks: []llm.Block{
			{Type: llm.BlockToolResult, ToolID: "call_1", Content: "no pods", IsError: false},
		}},
	}
	_, _, err := client.Complete(context.Background(), llm.Request{
		System:   "You are ahab.",
		Messages: history,
		Tools: []llm.ToolDef{{
			Name:        "cluster_read",
			Description: "Read cluster state",
			InputSchema: map[string]any{"args": map[string]any{"type": "array"}},
			Required:    []string{"args"},
		}},
		MaxTokens: 512,
	}, nil)
	if err != nil {
		t.Fatal(err)
	}

	var req struct {
		Model    string `json:"model"`
		Stream   bool   `json:"stream"`
		MaxTok   *int   `json:"max_tokens"`
		MaxComp  *int   `json:"max_completion_tokens"`
		Messages []struct {
			Role       string `json:"role"`
			Content    string `json:"content"`
			ToolCallID string `json:"tool_call_id"`
			ToolCalls  []struct {
				ID       string `json:"id"`
				Type     string `json:"type"`
				Function struct {
					Name      string `json:"name"`
					Arguments string `json:"arguments"`
				} `json:"function"`
			} `json:"tool_calls"`
		} `json:"messages"`
		Tools []struct {
			Type     string `json:"type"`
			Function struct {
				Name       string         `json:"name"`
				Parameters map[string]any `json:"parameters"`
			} `json:"function"`
		} `json:"tools"`
	}
	if err := json.Unmarshal(*captured, &req); err != nil {
		t.Fatal(err)
	}

	if !req.Stream || req.Model != "test-model" {
		t.Fatalf("model/stream = %q/%v", req.Model, req.Stream)
	}
	if req.MaxTok == nil || *req.MaxTok != 512 || req.MaxComp != nil {
		t.Fatalf("token params wrong: max_tokens=%v max_completion_tokens=%v", req.MaxTok, req.MaxComp)
	}

	// system, user, assistant(+tool_calls), tool
	roles := make([]string, len(req.Messages))
	for i, m := range req.Messages {
		roles[i] = m.Role
	}
	if strings.Join(roles, ",") != "system,user,assistant,tool" {
		t.Fatalf("roles = %v", roles)
	}
	asst := req.Messages[2]
	if strings.Contains(asst.Content, "must not be sent back") {
		t.Fatal("thinking content leaked into assistant message")
	}
	if len(asst.ToolCalls) != 1 || asst.ToolCalls[0].ID != "call_1" || asst.ToolCalls[0].Type != "function" {
		t.Fatalf("tool_calls = %+v", asst.ToolCalls)
	}
	if asst.ToolCalls[0].Function.Arguments != `{"args":["get","pods"]}` {
		t.Fatalf("arguments = %q", asst.ToolCalls[0].Function.Arguments)
	}
	tool := req.Messages[3]
	if tool.ToolCallID != "call_1" || tool.Content != "no pods" {
		t.Fatalf("tool message = %+v", tool)
	}

	if len(req.Tools) != 1 || req.Tools[0].Type != "function" || req.Tools[0].Function.Name != "cluster_read" {
		t.Fatalf("tools = %+v", req.Tools)
	}
	params := req.Tools[0].Function.Parameters
	if params["type"] != "object" || params["properties"] == nil || params["required"] == nil {
		t.Fatalf("parameters = %+v", params)
	}
}

func TestMaxCompletionTokensSwitch(t *testing.T) {
	client, captured := serve(t, []string{
		`data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}`,
		`data: [DONE]`,
	}, true)
	_, _, err := client.Complete(context.Background(), llm.Request{
		Messages: []llm.Message{llm.TextMessage(llm.RoleUser, "x")}, MaxTokens: 256,
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	var req map[string]any
	if err := json.Unmarshal(*captured, &req); err != nil {
		t.Fatal(err)
	}
	if _, has := req["max_tokens"]; has {
		t.Fatal("max_tokens sent despite UseMaxCompletionTokens")
	}
	if v, ok := req["max_completion_tokens"].(float64); !ok || v != 256 {
		t.Fatalf("max_completion_tokens = %v", req["max_completion_tokens"])
	}
}

func TestHTTPErrorSurfaced(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"error":{"message":"Invalid API key"}}`))
	}))
	defer srv.Close()
	client := New(Config{BaseURL: srv.URL, APIKey: "bad", Model: "m"})
	_, _, err := client.Complete(context.Background(), llm.Request{
		Messages: []llm.Message{llm.TextMessage(llm.RoleUser, "x")}, MaxTokens: 10,
	}, nil)
	if err == nil || !strings.Contains(err.Error(), "401") || !strings.Contains(err.Error(), "Invalid API key") {
		t.Fatalf("err = %v", err)
	}
}

func TestMidStreamErrorSurfaced(t *testing.T) {
	client, _ := serve(t, []string{
		`data: {"error":{"message":"quota exceeded","type":"insufficient_quota"}}`,
	}, false)
	_, _, err := client.Complete(context.Background(), llm.Request{
		Messages: []llm.Message{llm.TextMessage(llm.RoleUser, "x")}, MaxTokens: 10,
	}, nil)
	if err == nil || !strings.Contains(err.Error(), "quota exceeded") {
		t.Fatalf("err = %v", err)
	}
}
