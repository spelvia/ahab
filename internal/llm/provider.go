package llm

import "context"

// Provider is a language-model backend. Implementations stream incremental
// events to onEvent (which may be nil) and return the complete accumulated
// assistant message with the stop reason.
type Provider interface {
	Complete(ctx context.Context, req Request, onEvent func(Event)) (Message, StopReason, error)
}
