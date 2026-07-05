package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func setupRoots(t *testing.T) (*Roots, string, string) {
	t.Helper()
	work := t.TempDir()
	repo := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "deploy.yaml"), []byte("kind: Deployment\nreplicas: 2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(repo, "main.go"), []byte("package main // panic here\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	roots, err := NewRoots(work, repo)
	if err != nil {
		t.Fatal(err)
	}
	return roots, work, repo
}

func run(t *testing.T, tool *Tool, input any) (string, error) {
	t.Helper()
	raw, err := json.Marshal(input)
	if err != nil {
		t.Fatal(err)
	}
	return tool.Run(context.Background(), raw)
}

func TestReadFileWithinRoots(t *testing.T) {
	roots, _, repo := setupRoots(t)
	tool := ReadFile(roots)

	out, err := run(t, tool, map[string]string{"path": "deploy.yaml"})
	if err != nil || !strings.Contains(out, "replicas: 2") {
		t.Fatalf("relative read failed: %q, %v", out, err)
	}
	out, err = run(t, tool, map[string]string{"path": filepath.Join(repo, "main.go")})
	if err != nil || !strings.Contains(out, "panic here") {
		t.Fatalf("linked-repo read failed: %q, %v", out, err)
	}
}

func TestReadFileEscapesRejected(t *testing.T) {
	roots, _, _ := setupRoots(t)
	tool := ReadFile(roots)

	for _, path := range []string{"/etc/passwd", "../../../../etc/passwd"} {
		if _, err := run(t, tool, map[string]string{"path": path}); err == nil || !strings.Contains(err.Error(), "outside the allowed directories") {
			t.Errorf("path %q not rejected: %v", path, err)
		}
	}
}

func TestGrepFindsMatches(t *testing.T) {
	roots, _, _ := setupRoots(t)
	out, err := run(t, Grep(roots), map[string]string{"pattern": "replicas:"})
	if err != nil || !strings.Contains(out, "deploy.yaml:2") {
		t.Fatalf("grep failed: %q, %v", out, err)
	}
	out, err = run(t, Grep(roots), map[string]string{"pattern": "nomatchanywhere"})
	if err != nil || out != "(no matches)" {
		t.Fatalf("empty grep = %q, %v", out, err)
	}
}

func TestListFilesSkipsGit(t *testing.T) {
	roots, work, _ := setupRoots(t)
	if err := os.MkdirAll(filepath.Join(work, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(work, ".git", "HEAD"), []byte("ref"), 0o644); err != nil {
		t.Fatal(err)
	}
	out, err := run(t, ListFiles(roots), map[string]string{})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(out, "HEAD") || !strings.Contains(out, "deploy.yaml") {
		t.Fatalf("listing wrong: %q", out)
	}
}
