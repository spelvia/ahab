// Package anthropic adapts the Anthropic Go SDK to the neutral llm.Provider
// interface.
package anthropic

import (
	"context"
	"encoding/json"
	"fmt"

	sdk "github.com/anthropics/anthropic-sdk-go"

	"github.com/spelvia/ahab/internal/llm"
)

// Client is an llm.Provider backed by the Anthropic Messages API.
type Client struct {
	client sdk.Client
	model  sdk.Model
}

// New builds a Client for the given model ID. Credentials are resolved from
// the environment (ANTHROPIC_API_KEY or an `ant auth login` profile).
func New(model string) *Client {
	return &Client{client: sdk.NewClient(), model: sdk.Model(model)}
}

// Complete implements llm.Provider with a streaming request.
func (c *Client) Complete(ctx context.Context, req llm.Request, onEvent func(llm.Event)) (llm.Message, llm.StopReason, error) {
	params := sdk.MessageNewParams{
		Model:     c.model,
		MaxTokens: int64(req.MaxTokens),
		Thinking:  sdk.ThinkingConfigParamUnion{OfAdaptive: &sdk.ThinkingConfigAdaptiveParam{}},
		Messages:  toSDKMessages(req.Messages),
		Tools:     toSDKTools(req.Tools),
	}
	if req.System != "" {
		params.System = []sdk.TextBlockParam{{Text: req.System}}
	}

	stream := c.client.Messages.NewStreaming(ctx, params)
	acc := sdk.Message{}
	for stream.Next() {
		event := stream.Current()
		if err := acc.Accumulate(event); err != nil {
			return llm.Message{}, llm.StopOther, fmt.Errorf("accumulating stream: %w", err)
		}
		if onEvent == nil {
			continue
		}
		switch ev := event.AsAny().(type) {
		case sdk.ContentBlockStartEvent:
			if ev.ContentBlock.Type == "tool_use" {
				onEvent(llm.Event{Type: llm.EventToolUseStart, ToolName: ev.ContentBlock.Name})
			}
		case sdk.ContentBlockDeltaEvent:
			switch d := ev.Delta.AsAny().(type) {
			case sdk.TextDelta:
				onEvent(llm.Event{Type: llm.EventTextDelta, Text: d.Text})
			case sdk.ThinkingDelta:
				onEvent(llm.Event{Type: llm.EventThinkingDelta, Text: d.Thinking})
			}
		}
	}
	if err := stream.Err(); err != nil {
		return llm.Message{}, llm.StopOther, err
	}
	return fromSDKMessage(acc), fromSDKStopReason(acc.StopReason), nil
}

func toSDKMessages(msgs []llm.Message) []sdk.MessageParam {
	out := make([]sdk.MessageParam, 0, len(msgs))
	for _, m := range msgs {
		blocks := make([]sdk.ContentBlockParamUnion, 0, len(m.Blocks))
		for _, b := range m.Blocks {
			switch b.Type {
			case llm.BlockText:
				blocks = append(blocks, sdk.NewTextBlock(b.Text))
			case llm.BlockThinking:
				blocks = append(blocks, sdk.ContentBlockParamUnion{
					OfThinking: &sdk.ThinkingBlockParam{Thinking: b.Text, Signature: b.Signature},
				})
			case llm.BlockRedactedThinking:
				blocks = append(blocks, sdk.ContentBlockParamUnion{
					OfRedactedThinking: &sdk.RedactedThinkingBlockParam{Data: b.Signature},
				})
			case llm.BlockToolUse:
				blocks = append(blocks, sdk.ContentBlockParamUnion{
					OfToolUse: &sdk.ToolUseBlockParam{ID: b.ToolID, Name: b.ToolName, Input: b.Input},
				})
			case llm.BlockToolResult:
				blocks = append(blocks, sdk.NewToolResultBlock(b.ToolID, b.Content, b.IsError))
			}
		}
		role := sdk.MessageParamRoleUser
		if m.Role == llm.RoleAssistant {
			role = sdk.MessageParamRoleAssistant
		}
		out = append(out, sdk.MessageParam{Role: role, Content: blocks})
	}
	return out
}

func toSDKTools(tools []llm.ToolDef) []sdk.ToolUnionParam {
	out := make([]sdk.ToolUnionParam, 0, len(tools))
	for _, t := range tools {
		out = append(out, sdk.ToolUnionParam{OfTool: &sdk.ToolParam{
			Name:        t.Name,
			Description: sdk.String(t.Description),
			InputSchema: sdk.ToolInputSchemaParam{
				Properties: t.InputSchema,
				Required:   t.Required,
			},
		}})
	}
	return out
}

func fromSDKMessage(msg sdk.Message) llm.Message {
	out := llm.Message{Role: llm.RoleAssistant}
	for _, block := range msg.Content {
		switch v := block.AsAny().(type) {
		case sdk.TextBlock:
			out.Blocks = append(out.Blocks, llm.Block{Type: llm.BlockText, Text: v.Text})
		case sdk.ThinkingBlock:
			out.Blocks = append(out.Blocks, llm.Block{Type: llm.BlockThinking, Text: v.Thinking, Signature: v.Signature})
		case sdk.RedactedThinkingBlock:
			out.Blocks = append(out.Blocks, llm.Block{Type: llm.BlockRedactedThinking, Signature: v.Data})
		case sdk.ToolUseBlock:
			out.Blocks = append(out.Blocks, llm.Block{
				Type:     llm.BlockToolUse,
				ToolID:   v.ID,
				ToolName: v.Name,
				Input:    json.RawMessage(v.JSON.Input.Raw()),
			})
		}
	}
	return out
}

func fromSDKStopReason(r sdk.StopReason) llm.StopReason {
	switch r {
	case sdk.StopReasonEndTurn:
		return llm.StopEndTurn
	case sdk.StopReasonToolUse:
		return llm.StopToolUse
	case sdk.StopReasonMaxTokens:
		return llm.StopMaxTokens
	case sdk.StopReasonRefusal:
		return llm.StopRefusal
	default:
		return llm.StopOther
	}
}
