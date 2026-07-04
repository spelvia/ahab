// Package recorder persists the agent's command history as one JSONL file per
// session and reads it back as a tree for review and replay.
package recorder

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/spelvia/ahab/internal/agent"
)

// entry is one JSONL line: a session header or a tool record.
type entry struct {
	Type    string        `json:"type"` // "session" | "record"
	Session *Header       `json:"session,omitempty"`
	Record  *agent.Record `json:"record,omitempty"`
}

// Header describes a session.
type Header struct {
	ID     string    `json:"id"`
	Mode   string    `json:"mode"`
	Prompt string    `json:"prompt"`
	Start  time.Time `json:"start"`
}

// Recorder appends records for one session. It implements agent.Recorder.
type Recorder struct {
	mu  sync.Mutex
	f   *os.File
	enc *json.Encoder
	id  string
}

// New creates <dir>/<sessionID>.jsonl and writes the session header.
func New(dir, sessionID, mode, prompt string) (*Recorder, error) {
	f, err := os.OpenFile(filepath.Join(dir, sessionID+".jsonl"), os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0o644)
	if err != nil {
		return nil, err
	}
	r := &Recorder{f: f, enc: json.NewEncoder(f), id: sessionID}
	if err := r.enc.Encode(entry{Type: "session", Session: &Header{
		ID: sessionID, Mode: mode, Prompt: prompt, Start: time.Now(),
	}}); err != nil {
		f.Close()
		return nil, err
	}
	return r, nil
}

// SessionID returns the session this recorder writes to.
func (r *Recorder) SessionID() string { return r.id }

// Record implements agent.Recorder. Write errors are swallowed: recording
// must never break the session.
func (r *Recorder) Record(rec agent.Record) {
	r.mu.Lock()
	defer r.mu.Unlock()
	_ = r.enc.Encode(entry{Type: "record", Record: &rec})
	_ = r.f.Sync()
}

func (r *Recorder) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.f.Close()
}

// Session is a fully loaded history file.
type Session struct {
	Header  Header
	Records []agent.Record
}

// Load reads one session file.
func Load(dir, sessionID string) (*Session, error) {
	f, err := os.Open(filepath.Join(dir, sessionID+".jsonl"))
	if err != nil {
		return nil, err
	}
	defer f.Close()

	s := &Session{}
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 1024*1024), 16*1024*1024)
	for sc.Scan() {
		var e entry
		if err := json.Unmarshal(sc.Bytes(), &e); err != nil {
			return nil, fmt.Errorf("corrupt history line: %w", err)
		}
		switch {
		case e.Type == "session" && e.Session != nil:
			s.Header = *e.Session
		case e.Type == "record" && e.Record != nil:
			s.Records = append(s.Records, *e.Record)
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return s, nil
}

// List returns all session IDs in dir, oldest first.
func List(dir string) ([]string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var ids []string
	for _, e := range entries {
		if name, ok := strings.CutSuffix(e.Name(), ".jsonl"); ok {
			ids = append(ids, name)
		}
	}
	slices.Sort(ids)
	return ids, nil
}

// Latest returns the most recent session ID, or "" when none exist.
func Latest(dir string) (string, error) {
	ids, err := List(dir)
	if err != nil || len(ids) == 0 {
		return "", err
	}
	return ids[len(ids)-1], nil
}

// RenderTree formats a session as a phase-grouped command tree.
func RenderTree(s *Session) string {
	var b strings.Builder
	fmt.Fprintf(&b, "session %s  [%s]  %s\n", s.Header.ID, s.Header.Mode, s.Header.Start.Format(time.RFC3339))
	if s.Header.Prompt != "" {
		fmt.Fprintf(&b, "prompt: %s\n", s.Header.Prompt)
	}

	phase := "\x00" // sentinel so the first record always opens a group
	for _, rec := range s.Records {
		if rec.Phase != phase {
			phase = rec.Phase
			label := phase
			if label == "" {
				label = "(no phase)"
			}
			fmt.Fprintf(&b, "└─ %s\n", label)
		}
		status := "ok"
		switch {
		case rec.Approval == "denied":
			status = "DENIED"
		case rec.IsError:
			status = "ERROR"
		}
		fmt.Fprintf(&b, "   ├─ [%s] %-12s %s  (%s, %s)\n",
			rec.Time.Format("15:04:05"), rec.Tool, rec.Intent, rec.Approval, status)
	}
	return b.String()
}
