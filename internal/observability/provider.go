// Package observability defines the provider-agnostic interface through which
// ahab will sync with observability backends (Grafana, Prometheus, Datadog,
// ...). Observability mode is planned; no provider ships in v1 — this
// interface pins the contract so providers can be added without touching the
// agent core.
package observability

import "context"

// Dashboard is a provider-neutral description of a dashboard or view.
type Dashboard struct {
	ID    string
	Title string
	URL   string
}

// Query is a provider-neutral metrics/log query request.
type Query struct {
	// Expr is the provider-native query expression (PromQL, LogQL, ...).
	Expr string
	// RangeMinutes bounds the lookback window.
	RangeMinutes int
}

// QueryResult carries a rendered, model-readable result.
type QueryResult struct {
	// Text is a compact textual rendering suitable for an LLM tool result.
	Text string
}

// Provider is one observability backend. Implementations register themselves
// by name so config can select them (mirroring the LLM provider model).
type Provider interface {
	Name() string
	// Dashboards lists dashboards relevant to a workload or namespace.
	Dashboards(ctx context.Context, selector map[string]string) ([]Dashboard, error)
	// Query executes a metrics or log query.
	Query(ctx context.Context, q Query) (QueryResult, error)
}
