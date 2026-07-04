package agent

// UI receives live updates from the agent loop: streamed model text and tool
// activity. Implementations must be safe to call from the loop goroutine.
type UI interface {
	// AgentText delivers a streamed chunk of the assistant's visible text.
	AgentText(delta string)
	// Thinking delivers a streamed chunk of the assistant's reasoning summary.
	Thinking(delta string)
	// ToolStart announces that a tool call is about to run.
	ToolStart(tool, intent string)
	// ToolEnd reports a finished tool call with its (possibly truncated) output.
	ToolEnd(tool, output string, isError bool)
}

// NoUI discards all updates.
type NoUI struct{}

func (NoUI) AgentText(string)          {}
func (NoUI) Thinking(string)           {}
func (NoUI) ToolStart(string, string)  {}
func (NoUI) ToolEnd(string, string, bool) {}
