package tui

import (
	"context"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/spelvia/ahab/internal/agent"
	"github.com/spelvia/ahab/internal/recorder"
)

// state is what the bottom pane is doing.
type state int

const (
	stateRunning state = iota // agent turn in flight
	stateApproval             // waiting for y/n on a gated action
	stateFeedback             // collecting denial feedback text
	stateAsk                  // agent asked the user a question
	stateIdle                 // turn finished; follow-up input available
	stateDone                 // session over; q to quit
)

const maxDetailLines = 30

var (
	styleTool     = lipgloss.NewStyle().Foreground(lipgloss.Color("6"))
	styleErr      = lipgloss.NewStyle().Foreground(lipgloss.Color("1"))
	styleDim      = lipgloss.NewStyle().Foreground(lipgloss.Color("8"))
	styleUser     = lipgloss.NewStyle().Foreground(lipgloss.Color("3")).Bold(true)
	styleTitle    = lipgloss.NewStyle().Bold(true)
	styleApproval = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("3")).
			Padding(0, 1)
)

type model struct {
	opts   Options
	sess   Session
	cancel context.CancelFunc

	vp      viewport.Model
	input   textinput.Model
	spin    spinner.Model
	width   int
	height  int
	ready   bool
	showHistory bool

	state      state
	transcript strings.Builder
	records    []agent.Record
	pending    *msgApproval
	pendingAsk *msgAsk
	runErr     error
}

func newModel(opts Options, sess Session, cancel context.CancelFunc) *model {
	in := textinput.New()
	in.CharLimit = 4096
	sp := spinner.New(spinner.WithSpinner(spinner.Dot))
	return &model{opts: opts, sess: sess, cancel: cancel, input: in, spin: sp, state: stateRunning}
}

func (m *model) Init() tea.Cmd {
	m.appendUser(m.opts.InitialPrompt)
	return tea.Batch(m.spin.Tick, m.turnCmd(m.opts.InitialPrompt))
}

// turnCmd runs one conversation turn on the agent goroutine.
func (m *model) turnCmd(input string) tea.Cmd {
	return func() tea.Msg {
		return msgTurnDone{err: m.opts.Turn(m.opts.ctx, m.sess, input)}
	}
}

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.layout()
		return m, nil

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spin, cmd = m.spin.Update(msg)
		return m, cmd

	case msgAgentText:
		m.transcript.WriteString(string(msg))
		m.refresh()
		return m, nil

	case msgThinking:
		return m, nil // reasoning summaries are not rendered; spinner covers it

	case msgToolStart:
		m.appendLine(styleTool.Render("▸ " + msg.intent))
		return m, nil

	case msgToolEnd:
		if msg.isError {
			m.appendLine(styleErr.Render(indent(firstLines(msg.output, 6), "  ")))
		} else if out := strings.TrimSpace(msg.output); out != "" {
			m.appendLine(styleDim.Render(indent(firstLines(out, 6), "  ")))
		}
		return m, nil

	case msgRecord:
		m.records = append(m.records, agent.Record(msg))
		return m, nil

	case msgApproval:
		m.pending = &msg
		m.state = stateApproval
		return m, nil

	case msgAsk:
		m.pendingAsk = &msg
		m.state = stateAsk
		m.input.Placeholder = "answer (enter to send)"
		m.input.SetValue("")
		return m, m.input.Focus()

	case msgTurnDone:
		m.runErr = msg.err
		if msg.err != nil {
			m.appendLine(styleErr.Render("✗ " + msg.err.Error()))
		}
		if m.opts.FollowUp {
			m.state = stateIdle
			m.input.Placeholder = "follow-up instruction (enter to send, ctrl+c to quit)"
			m.input.SetValue("")
			return m, m.input.Focus()
		}
		m.state = stateDone
		return m, nil

	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m *model) handleKey(key tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch key.String() {
	case "ctrl+c":
		m.cancel()
		return m, tea.Quit
	case "tab":
		m.showHistory = !m.showHistory
		m.refresh()
		return m, nil
	}

	switch m.state {
	case stateApproval:
		switch key.String() {
		case "y":
			m.pending.resp <- agent.ApprovalResponse{Approved: true}
			m.appendLine(styleDim.Render("✓ approved"))
			m.pending = nil
			m.state = stateRunning
		case "n":
			m.state = stateFeedback
			m.input.Placeholder = "why? feedback for the agent (enter to send)"
			m.input.SetValue("")
			return m, m.input.Focus()
		}
		return m, nil

	case stateFeedback:
		switch key.String() {
		case "enter":
			m.pending.resp <- agent.ApprovalResponse{Approved: false, Feedback: m.input.Value()}
			m.appendLine(styleErr.Render("✗ denied: " + m.input.Value()))
			m.pending = nil
			m.input.Blur()
			m.state = stateRunning
			return m, nil
		case "esc":
			m.input.Blur()
			m.state = stateApproval
			return m, nil
		}

	case stateAsk:
		if key.String() == "enter" {
			answer := m.input.Value()
			m.pendingAsk.resp <- answer
			m.appendLine(styleUser.Render("you: ") + answer)
			m.pendingAsk = nil
			m.input.Blur()
			m.state = stateRunning
			return m, nil
		}

	case stateIdle:
		if key.String() == "enter" && strings.TrimSpace(m.input.Value()) != "" {
			input := m.input.Value()
			m.input.SetValue("")
			m.input.Blur()
			m.appendUser(input)
			m.state = stateRunning
			return m, m.turnCmd(input)
		}

	case stateDone:
		if key.String() == "q" {
			m.cancel()
			return m, tea.Quit
		}
	}

	// Route remaining keys to the focused input or the viewport.
	var cmd tea.Cmd
	if m.input.Focused() {
		m.input, cmd = m.input.Update(key)
	} else {
		m.vp, cmd = m.vp.Update(key)
	}
	return m, cmd
}

func (m *model) View() string {
	if !m.ready {
		return "starting…"
	}
	var b strings.Builder
	b.WriteString(styleTitle.Render(m.opts.Title) + styleDim.Render("  session "+m.opts.SessionID+"  (tab: history, ctrl+c: quit)") + "\n")
	b.WriteString(m.vp.View() + "\n")
	b.WriteString(m.statusLine() + "\n")
	b.WriteString(m.bottomPane())
	return b.String()
}

func (m *model) statusLine() string {
	switch m.state {
	case stateRunning:
		return m.spin.View() + styleDim.Render(" agent working…")
	case stateApproval, stateFeedback:
		return styleUser.Render("approval required")
	case stateAsk:
		return styleUser.Render("the agent has a question")
	case stateIdle:
		return styleDim.Render("turn complete — awaiting follow-up")
	case stateDone:
		if m.runErr != nil {
			return styleErr.Render("session ended with an error — press q to quit")
		}
		return styleDim.Render("session complete — press q to quit")
	}
	return ""
}

func (m *model) bottomPane() string {
	switch m.state {
	case stateApproval:
		req := m.pending.req
		body := styleTitle.Render(fmt.Sprintf("%s: %s", req.Tool, req.Intent))
		if detail := strings.TrimSpace(req.Detail); detail != "" && detail != req.Intent {
			body += "\n" + firstLines(detail, maxDetailLines)
		}
		body += "\n" + styleDim.Render("[y] approve   [n] deny with feedback")
		return styleApproval.Width(max(20, m.width-2)).Render(body)
	case stateFeedback, stateAsk, stateIdle:
		if m.state == stateAsk {
			return styleUser.Render("? "+m.pendingAsk.question) + "\n" + m.input.View()
		}
		return m.input.View()
	}
	return ""
}

func (m *model) layout() {
	bottomH := 4
	if m.state == stateApproval && m.pending != nil {
		bottomH = min(maxDetailLines+4, strings.Count(m.bottomPane(), "\n")+2)
	}
	h := max(3, m.height-bottomH-2)
	if !m.ready {
		m.vp = viewport.New(m.width, h)
		m.ready = true
	} else {
		m.vp.Width = m.width
		m.vp.Height = h
	}
	m.input.Width = max(20, m.width-4)
	m.refresh()
}

func (m *model) refresh() {
	if !m.ready {
		return
	}
	content := m.transcript.String()
	if m.showHistory {
		content = m.historyTree()
	}
	m.vp.SetContent(lipgloss.NewStyle().Width(max(20, m.width-1)).Render(content))
	m.vp.GotoBottom()
}

func (m *model) historyTree() string {
	s := &recorder.Session{
		Header:  recorder.Header{ID: m.opts.SessionID, Mode: m.opts.Mode, Prompt: m.opts.InitialPrompt},
		Records: m.records,
	}
	return recorder.RenderTree(s)
}

func (m *model) appendUser(text string) {
	m.appendLine(styleUser.Render("you: ") + text)
}

func (m *model) appendLine(line string) {
	if s := m.transcript.String(); s != "" && !strings.HasSuffix(s, "\n") {
		m.transcript.WriteString("\n")
	}
	m.transcript.WriteString(line + "\n")
	m.refresh()
}

func firstLines(s string, n int) string {
	lines := strings.Split(strings.TrimRight(s, "\n"), "\n")
	if len(lines) <= n {
		return strings.Join(lines, "\n")
	}
	return strings.Join(lines[:n], "\n") + styleDim.Render(fmt.Sprintf("\n… (%d more lines)", len(lines)-n))
}

func indent(s, prefix string) string {
	lines := strings.Split(s, "\n")
	for i := range lines {
		lines[i] = prefix + lines[i]
	}
	return strings.Join(lines, "\n")
}
