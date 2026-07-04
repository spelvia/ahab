package tools

import (
	"context"
	"encoding/json"

	"github.com/spelvia/ahab/internal/llm"
)

// AskFunc answers a question from the agent to the user, blocking until the
// user responds.
type AskFunc func(ctx context.Context, question string) (string, error)

// AskUser returns the ask_user tool backed by the given asker.
func AskUser(ask AskFunc) *Tool {
	type in struct {
		Question string `json:"question"`
	}
	return &Tool{
		Def: llm.ToolDef{
			Name:        "ask_user",
			Description: "Ask the user a clarifying question and wait for their answer. Use sparingly, for decisions only the user can make.",
			InputSchema: map[string]any{"question": map[string]any{"type": "string"}},
			Required:    []string{"question"},
		},
		Policy: PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = Input(raw, &i)
			return "ask: " + i.Question
		},
		Run: func(ctx context.Context, raw json.RawMessage) (string, error) {
			var i in
			if err := Input(raw, &i); err != nil {
				return "", err
			}
			return ask(ctx, i.Question)
		},
	}
}
