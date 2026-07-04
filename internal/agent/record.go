package agent

import "time"

// Record captures one executed (or denied) tool call for the session history.
type Record struct {
	Time     time.Time `json:"ts"`
	Phase    string    `json:"phase,omitempty"` // mode phase (plan/write/apply/...)
	Tool     string    `json:"tool"`
	Intent   string    `json:"intent"`
	Input    string    `json:"input"`
	Output   string    `json:"output"`
	IsError  bool      `json:"isError"`
	Approval string    `json:"approval"` // auto | approved | denied
	ParentID string    `json:"parentId,omitempty"`
}

// Recorder persists Records. Implementations must tolerate concurrent calls.
type Recorder interface {
	Record(r Record)
}

// NoRecorder discards records.
type NoRecorder struct{}

func (NoRecorder) Record(Record) {}
