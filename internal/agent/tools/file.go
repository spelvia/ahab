package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/spelvia/ahab/internal/llm"
)

const (
	maxReadBytes   = 256 * 1024
	maxListEntries = 500
	maxGrepMatches = 200
)

var skipDirs = map[string]bool{".git": true, "node_modules": true, "vendor": true, ".ahab": true}

// Roots confines model-supplied paths to a set of allowed directory trees
// (the project working directory plus linked repositories).
type Roots struct {
	primary string
	all     []string
}

// NewRoots resolves the primary working directory and extra roots to absolute
// paths. Relative model paths resolve against the primary root.
func NewRoots(primary string, extra ...string) (*Roots, error) {
	r := &Roots{}
	for i, dir := range append([]string{primary}, extra...) {
		abs, err := filepath.Abs(dir)
		if err != nil {
			return nil, err
		}
		if resolved, err := filepath.EvalSymlinks(abs); err == nil {
			abs = resolved
		}
		if i == 0 {
			r.primary = abs
		}
		r.all = append(r.all, abs)
	}
	return r, nil
}

// Primary returns the main working directory.
func (r *Roots) Primary() string { return r.primary }

// Resolve maps a model-supplied path to an absolute path, rejecting anything
// outside the allowed roots (including symlink escapes for existing paths).
func (r *Roots) Resolve(p string) (string, error) {
	if p == "" {
		p = "."
	}
	abs := p
	if !filepath.IsAbs(p) {
		abs = filepath.Join(r.primary, p)
	}
	abs = filepath.Clean(abs)
	// Resolve symlinks on the deepest existing ancestor so a link cannot
	// escape the root.
	if resolved, err := filepath.EvalSymlinks(abs); err == nil {
		abs = resolved
	}
	for _, root := range r.all {
		if abs == root || strings.HasPrefix(abs, root+string(filepath.Separator)) {
			return abs, nil
		}
	}
	return "", fmt.Errorf("path %q is outside the allowed directories (%s)", p, strings.Join(r.all, ", "))
}

// ReadFile returns the read_file tool.
func ReadFile(roots *Roots) *Tool {
	type in struct {
		Path string `json:"path"`
	}
	return &Tool{
		Def: llm.ToolDef{
			Name:        "read_file",
			Description: "Read a text file from the project or a linked repository.",
			InputSchema: map[string]any{"path": map[string]any{"type": "string", "description": "File path, relative to the project root or absolute within a linked repo"}},
			Required:    []string{"path"},
		},
		Policy: PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = Input(raw, &i)
			return "read " + i.Path
		},
		Run: func(_ context.Context, raw json.RawMessage) (string, error) {
			var i in
			if err := Input(raw, &i); err != nil {
				return "", err
			}
			path, err := roots.Resolve(i.Path)
			if err != nil {
				return "", err
			}
			data, err := os.ReadFile(path)
			if err != nil {
				return "", err
			}
			if len(data) > maxReadBytes {
				return string(data[:maxReadBytes]) + fmt.Sprintf("\n[... file truncated: %d of %d bytes shown]", maxReadBytes, len(data)), nil
			}
			return string(data), nil
		},
	}
}

// ListFiles returns the list_files tool.
func ListFiles(roots *Roots) *Tool {
	type in struct {
		Path string `json:"path"`
	}
	return &Tool{
		Def: llm.ToolDef{
			Name:        "list_files",
			Description: "Recursively list files under a directory in the project or a linked repository.",
			InputSchema: map[string]any{"path": map[string]any{"type": "string", "description": "Directory path; defaults to the project root"}},
		},
		Policy: PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = Input(raw, &i)
			if i.Path == "" {
				i.Path = "."
			}
			return "list files under " + i.Path
		},
		Run: func(_ context.Context, raw json.RawMessage) (string, error) {
			var i in
			if err := Input(raw, &i); err != nil {
				return "", err
			}
			dir, err := roots.Resolve(i.Path)
			if err != nil {
				return "", err
			}
			var lines []string
			err = filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
				if err != nil {
					return nil // unreadable entries are skipped, not fatal
				}
				if d.IsDir() {
					if skipDirs[d.Name()] {
						return filepath.SkipDir
					}
					return nil
				}
				rel, _ := filepath.Rel(dir, p)
				lines = append(lines, rel)
				if len(lines) >= maxListEntries {
					return filepath.SkipAll
				}
				return nil
			})
			if err != nil {
				return "", err
			}
			sort.Strings(lines)
			if len(lines) == 0 {
				return "(no files)", nil
			}
			out := strings.Join(lines, "\n")
			if len(lines) >= maxListEntries {
				out += fmt.Sprintf("\n[... listing truncated at %d entries]", maxListEntries)
			}
			return out, nil
		},
	}
}

// Grep returns the grep tool.
func Grep(roots *Roots) *Tool {
	type in struct {
		Pattern string `json:"pattern"`
		Path    string `json:"path"`
	}
	return &Tool{
		Def: llm.ToolDef{
			Name:        "grep",
			Description: "Search file contents with a Go regular expression; returns file:line matches.",
			InputSchema: map[string]any{
				"pattern": map[string]any{"type": "string", "description": "Go regexp to search for"},
				"path":    map[string]any{"type": "string", "description": "Directory or file to search; defaults to the project root"},
			},
			Required: []string{"pattern"},
		},
		Policy: PolicyAuto,
		Describe: func(raw json.RawMessage) string {
			var i in
			_ = Input(raw, &i)
			return fmt.Sprintf("grep %q", i.Pattern)
		},
		Run: func(_ context.Context, raw json.RawMessage) (string, error) {
			var i in
			if err := Input(raw, &i); err != nil {
				return "", err
			}
			re, err := regexp.Compile(i.Pattern)
			if err != nil {
				return "", fmt.Errorf("invalid pattern: %w", err)
			}
			start, err := roots.Resolve(i.Path)
			if err != nil {
				return "", err
			}
			var matches []string
			walk := func(p string, d fs.DirEntry, err error) error {
				if err != nil || d.IsDir() {
					if err == nil && d.IsDir() && skipDirs[d.Name()] {
						return filepath.SkipDir
					}
					return nil
				}
				data, err := os.ReadFile(p)
				if err != nil || isBinary(data) {
					return nil
				}
				rel, _ := filepath.Rel(roots.primary, p)
				for n, line := range strings.Split(string(data), "\n") {
					if re.MatchString(line) {
						matches = append(matches, fmt.Sprintf("%s:%d:%s", rel, n+1, strings.TrimSpace(line)))
						if len(matches) >= maxGrepMatches {
							return filepath.SkipAll
						}
					}
				}
				return nil
			}
			if err := filepath.WalkDir(start, walk); err != nil {
				return "", err
			}
			if len(matches) == 0 {
				return "(no matches)", nil
			}
			out := strings.Join(matches, "\n")
			if len(matches) >= maxGrepMatches {
				out += fmt.Sprintf("\n[... matches truncated at %d]", maxGrepMatches)
			}
			return out, nil
		},
	}
}

func isBinary(data []byte) bool {
	n := min(len(data), 512)
	for _, b := range data[:n] {
		if b == 0 {
			return true
		}
	}
	return false
}
