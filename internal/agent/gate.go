package agent

import "context"

// ApprovalRequest asks the user to allow or reject a gated action before it
// runs. Intent is the agent's one-line description; Detail is the full
// reviewable payload (exact command line, file diff, plan document).
type ApprovalRequest struct {
	Tool   string
	Intent string
	Detail string
}

// ApprovalResponse is the user's decision. Feedback accompanies a denial and
// is returned to the model so it can adjust its approach.
type ApprovalResponse struct {
	Approved bool
	Feedback string
}

// Gate decides whether a gated tool call may run. Interactive implementations
// block until the user answers; a future auto mode approves by policy.
type Gate interface {
	Confirm(ctx context.Context, req ApprovalRequest) (ApprovalResponse, error)
}

// GateFunc adapts a function to the Gate interface.
type GateFunc func(ctx context.Context, req ApprovalRequest) (ApprovalResponse, error)

func (f GateFunc) Confirm(ctx context.Context, req ApprovalRequest) (ApprovalResponse, error) {
	return f(ctx, req)
}
