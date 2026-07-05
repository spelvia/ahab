// Package llmtest provides a scripted fake llm.Provider for tests.
package llmtest

import (
	"context"
	"fmt"

	"github.com/spelvia/ahab/internal/llm"
)

// Turn is one scripted model response.
type Turn struct {
	Message llm.Message
	Stop    llm.StopReason
}

// Fake replays scripted turns in order and records every request it receives.
type Fake struct {
	Turns    []Turn
	Requests []llm.Request
	next     int
}

// TextTurn scripts a plain text response ending the turn.
func TextTurn(text string) Turn {
	return Turn{Message: llm.TextMessage(llm.RoleAssistant, text), Stop: llm.StopEndTurn}
}

// ToolTurn scripts a response that calls a single tool.
func ToolTurn(id, name, inputJSON string) Turn {
	return Turn{
		Message: llm.Message{Role: llm.RoleAssistant, Blocks: []llm.Block{{
			Type: llm.BlockToolUse, ToolID: id, ToolName: name, Input: []byte(inputJSON),
		}}},
		Stop: llm.StopToolUse,
	}
}

func (f *Fake) Complete(_ context.Context, req llm.Request, onEvent func(llm.Event)) (llm.Message, llm.StopReason, error) {
	f.Requests = append(f.Requests, req)
	if f.next >= len(f.Turns) {
		return llm.Message{}, llm.StopOther, fmt.Errorf("llmtest: no scripted turn for request %d", f.next+1)
	}
	turn := f.Turns[f.next]
	f.next++
	if onEvent != nil {
		for _, b := range turn.Message.Blocks {
			switch b.Type {
			case llm.BlockText:
				onEvent(llm.Event{Type: llm.EventTextDelta, Text: b.Text})
			case llm.BlockToolUse:
				onEvent(llm.Event{Type: llm.EventToolUseStart, ToolName: b.ToolName})
			}
		}
	}
	return turn.Message, turn.Stop, nil
}
