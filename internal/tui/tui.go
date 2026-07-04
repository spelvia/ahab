// Package tui is the interactive terminal frontend: it renders the agent's
// streamed output and tool activity, prompts for approvals with reviewable
// detail (commands, diffs, plans), answers agent questions, and shows the
// recorded session history.
package tui

import (
	"context"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/spelvia/ahab/internal/agent"
)

// Options configures one TUI session.
type Options struct {
	Title         string
	Mode          string
	SessionID     string
	InitialPrompt string
	// Turn runs one conversation turn; it is called with InitialPrompt first
	// and, when FollowUp is set, once per user follow-up.
	Turn func(ctx context.Context, s Session, input string) error
	// FollowUp enables the input box after a turn completes.
	FollowUp bool
	// Recorder receives every record in addition to the in-TUI history tab.
	Recorder agent.Recorder

	ctx context.Context
}

// Run starts the TUI and blocks until the session ends or the user quits.
func Run(ctx context.Context, opts Options) error {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	opts.ctx = ctx

	b := &bridge{tee: opts.Recorder}
	m := newModel(opts, b.session(), cancel)
	p := tea.NewProgram(m, tea.WithContext(ctx))
	b.p = p

	final, err := p.Run()
	if err != nil && ctx.Err() != nil {
		err = nil // user-initiated quit, not a failure
	}
	if err != nil {
		return err
	}
	if fm, ok := final.(*model); ok && fm.runErr != nil && ctx.Err() == nil {
		return fm.runErr
	}
	return nil
}
