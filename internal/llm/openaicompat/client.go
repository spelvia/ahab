// Package openaicompat adapts any OpenAI-compatible Chat Completions API to
// the neutral llm.Provider interface. It covers OpenAI itself plus providers
// that expose the same wire format, notably DeepSeek (api.deepseek.com) and
// Qwen via Alibaba DashScope's compatible mode.
//
// Wire format per the OpenAI API reference (POST {base}/chat/completions,
// SSE streaming, tools as {"type":"function",...}, tool results as
// {"role":"tool","tool_call_id":...}). DeepSeek and Qwen reasoning models
// additionally stream a reasoning_content delta, which is surfaced as
// thinking and never sent back.
package openaicompat

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"

	"github.com/spelvia/ahab/internal/llm"
)

// Config describes one OpenAI-compatible endpoint.
type Config struct {
	// BaseURL up to and excluding /chat/completions,
	// e.g. "https://api.deepseek.com/v1".
	BaseURL string
	APIKey  string
	Model   string
	// UseMaxCompletionTokens sends max_completion_tokens instead of
	// max_tokens (required by OpenAI reasoning models; other providers
	// still expect max_tokens).
	UseMaxCompletionTokens bool
}

// Client is an llm.Provider backed by a Chat Completions endpoint.
type Client struct {
	cfg  Config
	http *http.Client
}

// New builds a Client. The zero http timeout is deliberate: responses stream
// and are bounded by the caller's context.
func New(cfg Config) *Client {
	cfg.BaseURL = strings.TrimSuffix(cfg.BaseURL, "/")
	return &Client{cfg: cfg, http: &http.Client{}}
}

// --- request wire types ---

type chatRequest struct {
	Model               string        `json:"model"`
	Messages            []chatMessage `json:"messages"`
	Tools               []chatTool    `json:"tools,omitempty"`
	Stream              bool          `json:"stream"`
	MaxTokens           int           `json:"max_tokens,omitempty"`
	MaxCompletionTokens int           `json:"max_completion_tokens,omitempty"`
}

type chatMessage struct {
	Role       string     `json:"role"`
	Content    string     `json:"content"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
	ToolCalls  []toolCall `json:"tool_calls,omitempty"`
}

type toolCall struct {
	ID       string       `json:"id"`
	Type     string       `json:"type"`
	Function functionCall `json:"function"`
}

type functionCall struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}

type chatTool struct {
	Type     string       `json:"type"`
	Function chatFunction `json:"function"`
}

type chatFunction struct {
	Name        string         `json:"name"`
	Description string         `json:"description,omitempty"`
	Parameters  map[string]any `json:"parameters"`
}

// --- streaming wire types ---

type streamChunk struct {
	Choices []struct {
		Delta struct {
			Content          string `json:"content"`
			ReasoningContent string `json:"reasoning_content"`
			ToolCalls        []struct {
				Index    int    `json:"index"`
				ID       string `json:"id"`
				Function struct {
					Name      string `json:"name"`
					Arguments string `json:"arguments"`
				} `json:"function"`
			} `json:"tool_calls"`
		} `json:"delta"`
		FinishReason string `json:"finish_reason"`
	} `json:"choices"`
	Error *apiError `json:"error"`
}

type apiError struct {
	Message string `json:"message"`
	Type    string `json:"type"`
}

// Complete implements llm.Provider with a streaming request.
func (c *Client) Complete(ctx context.Context, req llm.Request, onEvent func(llm.Event)) (llm.Message, llm.StopReason, error) {
	body := chatRequest{
		Model:    c.cfg.Model,
		Messages: toChatMessages(req.System, req.Messages),
		Tools:    toChatTools(req.Tools),
		Stream:   true,
	}
	if c.cfg.UseMaxCompletionTokens {
		body.MaxCompletionTokens = req.MaxTokens
	} else {
		body.MaxTokens = req.MaxTokens
	}
	payload, err := json.Marshal(body)
	if err != nil {
		return llm.Message{}, llm.StopOther, err
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, c.cfg.BaseURL+"/chat/completions", bytes.NewReader(payload))
	if err != nil {
		return llm.Message{}, llm.StopOther, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Authorization", "Bearer "+c.cfg.APIKey)
	httpReq.Header.Set("Accept", "text/event-stream")

	resp, err := c.http.Do(httpReq)
	if err != nil {
		return llm.Message{}, llm.StopOther, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		detail, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
		return llm.Message{}, llm.StopOther, fmt.Errorf("%s returned HTTP %d: %s", c.cfg.BaseURL, resp.StatusCode, strings.TrimSpace(string(detail)))
	}
	return c.readStream(resp.Body, onEvent)
}

// pendingToolCall accumulates one streamed tool call across chunks.
type pendingToolCall struct {
	index int
	id    string
	name  string
	args  strings.Builder
}

func (c *Client) readStream(r io.Reader, onEvent func(llm.Event)) (llm.Message, llm.StopReason, error) {
	var (
		text         strings.Builder
		reasoning    strings.Builder
		toolCalls    = map[int]*pendingToolCall{}
		finishReason string
	)

	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		data, ok := strings.CutPrefix(line, "data:")
		if !ok {
			continue // blank keep-alive lines and SSE comments
		}
		data = strings.TrimSpace(data)
		if data == "[DONE]" {
			break
		}

		var chunk streamChunk
		if err := json.Unmarshal([]byte(data), &chunk); err != nil {
			return llm.Message{}, llm.StopOther, fmt.Errorf("malformed stream chunk: %w", err)
		}
		if chunk.Error != nil {
			return llm.Message{}, llm.StopOther, fmt.Errorf("provider error mid-stream: %s", chunk.Error.Message)
		}
		if len(chunk.Choices) == 0 {
			continue // e.g. trailing usage-only chunk
		}
		choice := chunk.Choices[0]
		if choice.FinishReason != "" {
			finishReason = choice.FinishReason
		}
		if choice.Delta.Content != "" {
			text.WriteString(choice.Delta.Content)
			if onEvent != nil {
				onEvent(llm.Event{Type: llm.EventTextDelta, Text: choice.Delta.Content})
			}
		}
		if choice.Delta.ReasoningContent != "" {
			reasoning.WriteString(choice.Delta.ReasoningContent)
			if onEvent != nil {
				onEvent(llm.Event{Type: llm.EventThinkingDelta, Text: choice.Delta.ReasoningContent})
			}
		}
		for _, tc := range choice.Delta.ToolCalls {
			p := toolCalls[tc.Index]
			if p == nil {
				p = &pendingToolCall{index: tc.Index}
				toolCalls[tc.Index] = p
			}
			if tc.ID != "" {
				p.id = tc.ID
			}
			if tc.Function.Name != "" {
				if p.name == "" && onEvent != nil {
					onEvent(llm.Event{Type: llm.EventToolUseStart, ToolName: tc.Function.Name})
				}
				p.name = tc.Function.Name
			}
			p.args.WriteString(tc.Function.Arguments)
		}
	}
	if err := sc.Err(); err != nil {
		return llm.Message{}, llm.StopOther, err
	}

	msg := llm.Message{Role: llm.RoleAssistant}
	if reasoning.Len() > 0 {
		// Compat providers' reasoning is display-only: it carries no
		// signature and is never sent back (see toChatMessages).
		msg.Blocks = append(msg.Blocks, llm.Block{Type: llm.BlockThinking, Text: reasoning.String()})
	}
	if text.Len() > 0 {
		msg.Blocks = append(msg.Blocks, llm.Block{Type: llm.BlockText, Text: text.String()})
	}
	ordered := make([]*pendingToolCall, 0, len(toolCalls))
	for _, p := range toolCalls {
		ordered = append(ordered, p)
	}
	sort.Slice(ordered, func(i, j int) bool { return ordered[i].index < ordered[j].index })
	for _, p := range ordered {
		args := p.args.String()
		if strings.TrimSpace(args) == "" {
			args = "{}"
		}
		msg.Blocks = append(msg.Blocks, llm.Block{
			Type:     llm.BlockToolUse,
			ToolID:   p.id,
			ToolName: p.name,
			Input:    json.RawMessage(args),
		})
	}

	stop := fromFinishReason(finishReason)
	if len(ordered) > 0 {
		// Some compat providers report finish_reason "stop" even when the
		// message contains tool calls; the calls themselves are authoritative.
		stop = llm.StopToolUse
	}
	return msg, stop, nil
}

func toChatMessages(system string, msgs []llm.Message) []chatMessage {
	out := make([]chatMessage, 0, len(msgs)+1)
	if system != "" {
		out = append(out, chatMessage{Role: "system", Content: system})
	}
	for _, m := range msgs {
		switch m.Role {
		case llm.RoleUser:
			var texts []string
			for _, b := range m.Blocks {
				switch b.Type {
				case llm.BlockToolResult:
					content := b.Content
					if b.IsError {
						content = "ERROR: " + content
					}
					out = append(out, chatMessage{Role: "tool", ToolCallID: b.ToolID, Content: content})
				case llm.BlockText:
					texts = append(texts, b.Text)
				}
			}
			if len(texts) > 0 {
				out = append(out, chatMessage{Role: "user", Content: strings.Join(texts, "\n")})
			}
		case llm.RoleAssistant:
			cm := chatMessage{Role: "assistant"}
			var texts []string
			for _, b := range m.Blocks {
				switch b.Type {
				case llm.BlockText:
					texts = append(texts, b.Text)
				case llm.BlockToolUse:
					cm.ToolCalls = append(cm.ToolCalls, toolCall{
						ID:   b.ToolID,
						Type: "function",
						Function: functionCall{
							Name:      b.ToolName,
							Arguments: string(b.Input),
						},
					})
					// Thinking blocks are intentionally dropped: DeepSeek and
					// Qwen document that reasoning_content must not be sent
					// back as context.
				}
			}
			cm.Content = strings.Join(texts, "\n")
			out = append(out, cm)
		}
	}
	return out
}

func toChatTools(tools []llm.ToolDef) []chatTool {
	out := make([]chatTool, 0, len(tools))
	for _, t := range tools {
		params := map[string]any{
			"type":       "object",
			"properties": t.InputSchema,
		}
		if len(t.Required) > 0 {
			params["required"] = t.Required
		}
		out = append(out, chatTool{
			Type: "function",
			Function: chatFunction{
				Name:        t.Name,
				Description: t.Description,
				Parameters:  params,
			},
		})
	}
	return out
}

func fromFinishReason(r string) llm.StopReason {
	switch r {
	case "stop":
		return llm.StopEndTurn
	case "tool_calls", "function_call":
		return llm.StopToolUse
	case "length":
		return llm.StopMaxTokens
	case "content_filter":
		return llm.StopRefusal
	default:
		return llm.StopOther
	}
}
