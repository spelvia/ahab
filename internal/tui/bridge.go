package tui

import (
	"context"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/agent/tools"
)

// Session bundles the agent-facing callbacks a mode needs, all backed by the
// running TUI program.
type Session struct {
	UI       agent.UI
	Gate     agent.Gate
	Ask      tools.AskFunc
	Recorder agent.Recorder
}

// Messages sent from the agent goroutine into the Bubble Tea program.
type (
	msgAgentText string
	msgThinking  string
	msgToolStart struct{ tool, intent string }
	msgToolEnd   struct {
		tool, output string
		isError      bool
	}
	msgRecord   agent.Record
	msgApproval struct {
		req  agent.ApprovalRequest
		resp chan agent.ApprovalResponse
	}
	msgAsk struct {
		question string
		resp     chan string
	}
	msgTurnDone struct{ err error }
)

// bridge implements agent.UI, agent.Gate, tools.AskFunc, and agent.Recorder
// by forwarding into the TUI event loop. Gate and Ask block the agent
// goroutine until the user answers.
type bridge struct {
	p   *tea.Program
	tee agent.Recorder // file recorder; may be nil
}

func (b *bridge) session() Session {
	return Session{UI: b, Gate: b, Ask: b.ask, Recorder: b}
}

func (b *bridge) AgentText(delta string) { b.p.Send(msgAgentText(delta)) }
func (b *bridge) Thinking(delta string)  { b.p.Send(msgThinking(delta)) }
func (b *bridge) ToolStart(tool, intent string) {
	b.p.Send(msgToolStart{tool: tool, intent: intent})
}
func (b *bridge) ToolEnd(tool, output string, isError bool) {
	b.p.Send(msgToolEnd{tool: tool, output: output, isError: isError})
}

func (b *bridge) Record(r agent.Record) {
	if b.tee != nil {
		b.tee.Record(r)
	}
	b.p.Send(msgRecord(r))
}

func (b *bridge) Confirm(ctx context.Context, req agent.ApprovalRequest) (agent.ApprovalResponse, error) {
	resp := make(chan agent.ApprovalResponse, 1)
	b.p.Send(msgApproval{req: req, resp: resp})
	select {
	case r := <-resp:
		return r, nil
	case <-ctx.Done():
		return agent.ApprovalResponse{}, ctx.Err()
	}
}

func (b *bridge) ask(ctx context.Context, question string) (string, error) {
	resp := make(chan string, 1)
	b.p.Send(msgAsk{question: question, resp: resp})
	select {
	case r := <-resp:
		return r, nil
	case <-ctx.Done():
		return "", ctx.Err()
	}
}
