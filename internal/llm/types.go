// Package llm defines the provider-neutral types and interface through which
// the agent talks to a language model. The agent loop and modes depend only on
// this package; provider SDKs live in subpackages (e.g. llm/anthropic).
package llm

import "encoding/json"

// Role identifies the author of a message.
type Role string

const (
	RoleUser      Role = "user"
	RoleAssistant Role = "assistant"
)

// BlockType discriminates the variants of a content Block.
type BlockType string

const (
	BlockText             BlockType = "text"
	BlockThinking         BlockType = "thinking"
	BlockRedactedThinking BlockType = "redacted_thinking"
	BlockToolUse          BlockType = "tool_use"
	BlockToolResult       BlockType = "tool_result"
)

// Block is one content block of a message. Exactly the fields relevant to its
// Type are set.
type Block struct {
	Type BlockType

	// Text carries the content for BlockText and BlockThinking.
	Text string
	// Signature is the opaque thinking-block signature; it must be echoed
	// back unchanged when the conversation continues on the same model.
	Signature string

	// ToolUse fields (BlockToolUse).
	ToolID   string
	ToolName string
	Input    json.RawMessage

	// ToolResult fields (BlockToolResult). ToolID references the tool_use ID.
	Content string
	IsError bool
}

// Message is a single conversation turn.
type Message struct {
	Role   Role
	Blocks []Block
}

// TextMessage builds a single-text-block message.
func TextMessage(role Role, text string) Message {
	return Message{Role: role, Blocks: []Block{{Type: BlockText, Text: text}}}
}

// ToolUses returns the tool_use blocks of a message, in order.
func (m Message) ToolUses() []Block {
	var uses []Block
	for _, b := range m.Blocks {
		if b.Type == BlockToolUse {
			uses = append(uses, b)
		}
	}
	return uses
}

// Text concatenates the message's text blocks.
func (m Message) Text() string {
	var s string
	for _, b := range m.Blocks {
		if b.Type == BlockText {
			s += b.Text
		}
	}
	return s
}

// ToolDef declares a tool the model may call. InputSchema is the JSON-schema
// "properties" object; Required lists the mandatory property names.
type ToolDef struct {
	Name        string
	Description string
	InputSchema map[string]any
	Required    []string
}

// Request is one model invocation.
type Request struct {
	System    string
	Messages  []Message
	Tools     []ToolDef
	MaxTokens int
}

// StopReason reports why the model stopped generating.
type StopReason string

const (
	StopEndTurn   StopReason = "end_turn"
	StopToolUse   StopReason = "tool_use"
	StopMaxTokens StopReason = "max_tokens"
	StopRefusal   StopReason = "refusal"
	StopOther     StopReason = "other"
)

// EventType discriminates streaming events.
type EventType string

const (
	EventTextDelta     EventType = "text_delta"
	EventThinkingDelta EventType = "thinking_delta"
	EventToolUseStart  EventType = "tool_use_start"
)

// Event is a streaming update emitted while the model generates.
type Event struct {
	Type     EventType
	Text     string // delta text for text/thinking deltas
	ToolName string // set for EventToolUseStart
}
