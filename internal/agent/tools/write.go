package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/spelvia/ahab/internal/llm"
)

// maxDiffBytes caps file sizes considered for diff rendering in approvals.
const maxDiffBytes = 200 * 1024

// WriteFile returns the gated write_file tool. Writes are confined to the
// project working directory (never linked repositories); the approval prompt
// shows a diff against the current file content.
func WriteFile(work *Roots) *Tool {
	type in struct {
		Path    string `json:"path"`
		Content string `json:"content"`
	}
	decode := func(raw json.RawMessage) (in, string, error) {
		var i in
		if err := Input(raw, &i); err != nil {
			return i, "", err
		}
		path, err := work.Resolve(i.Path)
		if err != nil {
			// A new file's parent may not exist yet; resolve the parent.
			parent, perr := work.Resolve(filepath.Dir(i.Path))
			if perr != nil {
				return i, "", err
			}
			path = filepath.Join(parent, filepath.Base(i.Path))
		}
		return i, path, nil
	}

	return &Tool{
		Def: llm.ToolDef{
			Name:        "write_file",
			Description: "Create or overwrite a file in the project directory (manifests, Helm templates, values files). The user reviews the diff before it is written.",
			InputSchema: map[string]any{
				"path":    map[string]any{"type": "string", "description": "File path relative to the project root"},
				"content": map[string]any{"type": "string", "description": "Complete new file content"},
			},
			Required: []string{"path", "content"},
		},
		Policy: PolicyAsk,
		Phase:  "write",
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = Input(raw, &i)
			return "write " + i.Path
		},
		Detail: func(raw json.RawMessage) string {
			i, path, err := decode(raw)
			if err != nil {
				return "invalid input: " + err.Error()
			}
			old := ""
			if data, err := os.ReadFile(path); err == nil && len(data) <= maxDiffBytes {
				old = string(data)
			}
			if len(i.Content) > maxDiffBytes {
				return fmt.Sprintf("(file too large to diff: %d bytes)", len(i.Content))
			}
			return unifiedDiff(old, i.Content)
		},
		Run: func(_ context.Context, raw json.RawMessage) (string, error) {
			i, path, err := decode(raw)
			if err != nil {
				return "", err
			}
			if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
				return "", err
			}
			if err := os.WriteFile(path, []byte(i.Content), 0o644); err != nil {
				return "", err
			}
			return fmt.Sprintf("wrote %s (%d bytes)", i.Path, len(i.Content)), nil
		},
	}
}
