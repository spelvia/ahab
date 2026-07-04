// Package agent implements the manual agentic loop: stream a model response,
// route tool calls through the approval gate, execute them, and feed results
// back until the model ends its turn.
package agent

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/spelvia/ahab/internal/agent/tools"
	"github.com/spelvia/ahab/internal/llm"
)

// maxToolResultBytes caps how much tool output is returned to the model.
// The full output is always recorded.
const maxToolResultBytes = 48 * 1024

// maxIterations bounds model round-trips per Run call.
const maxIterations = 100

// ErrRefused is returned when the model declines the request.
var ErrRefused = errors.New("the model refused this request")

// Loop drives one agent conversation.
type Loop struct {
	Provider  llm.Provider
	Registry  *tools.Registry
	Gate      Gate
	UI        UI
	Recorder  Recorder
	System    string
	MaxTokens int

	// Phase labels records with the current mode phase (plan/write/apply/...).
	Phase string

	messages []llm.Message
}

// Messages exposes the conversation history (for tests and session dumps).
func (l *Loop) Messages() []llm.Message { return l.messages }

// Run appends a user message and loops model↔tools until the turn ends.
func (l *Loop) Run(ctx context.Context, userInput string) error {
	if l.UI == nil {
		l.UI = NoUI{}
	}
	if l.Recorder == nil {
		l.Recorder = NoRecorder{}
	}
	l.messages = append(l.messages, llm.TextMessage(llm.RoleUser, userInput))

	for i := 0; i < maxIterations; i++ {
		msg, stop, err := l.Provider.Complete(ctx, llm.Request{
			System:    l.System,
			Messages:  l.messages,
			Tools:     l.Registry.Defs(),
			MaxTokens: l.MaxTokens,
		}, l.forward)
		if err != nil {
			return err
		}
		l.messages = append(l.messages, msg)

		switch stop {
		case llm.StopRefusal:
			return ErrRefused
		case llm.StopMaxTokens:
			return errors.New("response truncated: max_tokens reached")
		}

		uses := msg.ToolUses()
		if len(uses) == 0 {
			return nil // end_turn
		}

		results := make([]llm.Block, 0, len(uses))
		for _, use := range uses {
			content, isErr := l.execute(ctx, use)
			if err := ctx.Err(); err != nil {
				return err
			}
			results = append(results, llm.Block{
				Type:    llm.BlockToolResult,
				ToolID:  use.ToolID,
				Content: content,
				IsError: isErr,
			})
		}
		l.messages = append(l.messages, llm.Message{Role: llm.RoleUser, Blocks: results})
	}
	return fmt.Errorf("agent did not finish within %d model turns", maxIterations)
}

func (l *Loop) forward(ev llm.Event) {
	switch ev.Type {
	case llm.EventTextDelta:
		l.UI.AgentText(ev.Text)
	case llm.EventThinkingDelta:
		l.UI.Thinking(ev.Text)
	}
}

// execute runs one tool call and returns the tool_result content.
func (l *Loop) execute(ctx context.Context, use llm.Block) (content string, isError bool) {
	rec := Record{
		Time:  time.Now(),
		Phase: l.Phase,
		Tool:  use.ToolName,
		Input: string(use.Input),
	}

	tool := l.Registry.Get(use.ToolName)
	if tool == nil {
		rec.Output = "unknown tool"
		rec.IsError = true
		rec.Approval = "auto"
		l.Recorder.Record(rec)
		return fmt.Sprintf("unknown tool %q", use.ToolName), true
	}

	rec.Intent = tool.Intent(use.Input)
	rec.Approval = "auto"

	if tool.Policy == tools.PolicyAsk {
		detail := ""
		if tool.Detail != nil {
			detail = tool.Detail(use.Input)
		}
		resp, err := l.Gate.Confirm(ctx, ApprovalRequest{
			Tool:   use.ToolName,
			Intent: rec.Intent,
			Detail: detail,
		})
		if err != nil {
			rec.Output = "approval aborted: " + err.Error()
			rec.IsError = true
			rec.Approval = "denied"
			l.Recorder.Record(rec)
			return rec.Output, true
		}
		if !resp.Approved {
			rec.Approval = "denied"
			rec.IsError = true
			rec.Output = "denied by user"
			l.Recorder.Record(rec)
			msg := "The user denied this action."
			if resp.Feedback != "" {
				msg += " Feedback: " + resp.Feedback
			}
			return msg, true
		}
		rec.Approval = "approved"
	}

	l.UI.ToolStart(use.ToolName, rec.Intent)
	out, err := tool.Run(ctx, use.Input)
	if err != nil {
		out = strAppend(out, "error: "+err.Error())
		isError = true
	}
	rec.Output = out
	rec.IsError = isError
	l.Recorder.Record(rec)
	l.UI.ToolEnd(use.ToolName, out, isError)

	return truncate(out, maxToolResultBytes), isError
}

func strAppend(a, b string) string {
	if a == "" {
		return b
	}
	return a + "\n" + b
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + fmt.Sprintf("\n[... output truncated: %d of %d bytes shown]", n, len(s))
}
