// Package tools defines the agent's tool registry: each tool couples a model
// -facing definition with an execution handler and an approval policy.
package tools

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/spelvia/ahab/internal/llm"
)

// Policy controls whether a tool call needs interactive approval.
type Policy string

const (
	// PolicyAuto runs without asking (still recorded).
	PolicyAuto Policy = "auto"
	// PolicyAsk blocks on user approval before every invocation.
	PolicyAsk Policy = "ask"
)

// Tool is a callable capability exposed to the model.
type Tool struct {
	Def    llm.ToolDef
	Policy Policy
	// Describe renders a one-line human-readable intent for approval prompts
	// and the activity log. May be nil.
	Describe func(input json.RawMessage) string
	// Detail renders the full reviewable payload (exact command, file diff).
	// May be nil; used by approval prompts.
	Detail func(input json.RawMessage) string
	// Run executes the tool and returns the tool_result content.
	Run func(ctx context.Context, input json.RawMessage) (string, error)
}

// Intent returns the human-readable description of a call.
func (t *Tool) Intent(input json.RawMessage) string {
	if t.Describe != nil {
		return t.Describe(input)
	}
	return t.Def.Name
}

// Registry holds the tools available to one agent session.
type Registry struct {
	byName map[string]*Tool
	order  []string
}

func NewRegistry(tools ...*Tool) *Registry {
	r := &Registry{byName: map[string]*Tool{}}
	for _, t := range tools {
		r.Add(t)
	}
	return r
}

func (r *Registry) Add(t *Tool) {
	if _, dup := r.byName[t.Def.Name]; dup {
		panic(fmt.Sprintf("tools: duplicate tool %q", t.Def.Name))
	}
	r.byName[t.Def.Name] = t
	r.order = append(r.order, t.Def.Name)
}

func (r *Registry) Get(name string) *Tool {
	return r.byName[name]
}

// Defs returns the model-facing tool definitions in registration order.
func (r *Registry) Defs() []llm.ToolDef {
	defs := make([]llm.ToolDef, 0, len(r.order))
	for _, name := range r.order {
		defs = append(defs, r.byName[name].Def)
	}
	return defs
}

// Input decodes a tool input JSON payload into dst, tolerating an empty input.
func Input(raw json.RawMessage, dst any) error {
	if len(raw) == 0 {
		return nil
	}
	return json.Unmarshal(raw, dst)
}
