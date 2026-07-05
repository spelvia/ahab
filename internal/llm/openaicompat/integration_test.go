package openaicompat_test

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/agent/tools"
	"github.com/spelvia/ahab/internal/llm"
	"github.com/spelvia/ahab/internal/llm/openaicompat"
)

// TestAgentLoopRoundTrip drives the real agent loop through the compat
// client: turn 1 streams a tool call, the loop executes the tool, and turn 2
// must receive the result as a role:"tool" message.
func TestAgentLoopRoundTrip(t *testing.T) {
	var bodies []json.RawMessage
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body json.RawMessage
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Errorf("decoding request %d: %v", len(bodies)+1, err)
		}
		bodies = append(bodies, body)
		w.Header().Set("Content-Type", "text/event-stream")
		switch len(bodies) {
		case 1:
			fmt.Fprint(w, `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"{\"message\":\"ahoy\"}"}}]}}]}`+"\n\n")
			fmt.Fprint(w, `data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}`+"\n\n")
		default:
			fmt.Fprint(w, `data: {"choices":[{"delta":{"content":"done"},"finish_reason":"stop"}]}`+"\n\n")
		}
		fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	echo := &tools.Tool{
		Def: llm.ToolDef{
			Name:        "echo",
			InputSchema: map[string]any{"message": map[string]any{"type": "string"}},
			Required:    []string{"message"},
		},
		Policy: tools.PolicyAuto,
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

	loop := &agent.Loop{
		Provider:  openaicompat.New(openaicompat.Config{BaseURL: srv.URL, APIKey: "k", Model: "m"}),
		Registry:  tools.NewRegistry(echo),
		MaxTokens: 100,
	}
	if err := loop.Run(context.Background(), "say ahoy"); err != nil {
		t.Fatal(err)
	}

	if len(bodies) != 2 {
		t.Fatalf("got %d requests, want 2", len(bodies))
	}
	var second struct {
		Messages []struct {
			Role       string `json:"role"`
			Content    string `json:"content"`
			ToolCallID string `json:"tool_call_id"`
		} `json:"messages"`
	}
	if err := json.Unmarshal(bodies[1], &second); err != nil {
		t.Fatal(err)
	}
	last := second.Messages[len(second.Messages)-1]
	if last.Role != "tool" || last.ToolCallID != "call_1" || last.Content != "echo: ahoy" {
		t.Fatalf("tool result message = %+v", last)
	}
}
