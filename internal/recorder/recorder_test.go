package recorder

import (
	"strings"
	"testing"
	"time"

	"github.com/spelvia/ahab/internal/agent"
)

func TestRoundTrip(t *testing.T) {
	dir := t.TempDir()

	rec, err := New(dir, "20260704-120000-abcd", "build", "deploy nginx")
	if err != nil {
		t.Fatal(err)
	}
	rec.Record(agent.Record{Time: time.Now(), Phase: "plan", Tool: "cluster_read", Intent: "list pods", Approval: "auto"})
	rec.Record(agent.Record{Time: time.Now(), Phase: "apply", Tool: "run_command", Intent: "kubectl apply deploy.yaml", Approval: "approved"})
	rec.Record(agent.Record{Time: time.Now(), Phase: "apply", Tool: "run_command", Intent: "kubectl delete ns prod", Approval: "denied", IsError: true})
	if err := rec.Close(); err != nil {
		t.Fatal(err)
	}

	s, err := Load(dir, "20260704-120000-abcd")
	if err != nil {
		t.Fatal(err)
	}
	if s.Header.Mode != "build" || s.Header.Prompt != "deploy nginx" {
		t.Fatalf("header = %+v", s.Header)
	}
	if len(s.Records) != 3 {
		t.Fatalf("got %d records, want 3", len(s.Records))
	}
	if s.Records[2].Approval != "denied" {
		t.Fatalf("record 2 = %+v", s.Records[2])
	}

	tree := RenderTree(s)
	for _, want := range []string{"session 20260704-120000-abcd", "plan", "apply", "DENIED", "kubectl apply deploy.yaml"} {
		if !strings.Contains(tree, want) {
			t.Errorf("tree missing %q:\n%s", want, tree)
		}
	}
}

func TestListAndLatest(t *testing.T) {
	dir := t.TempDir()

	if latest, err := Latest(dir); err != nil || latest != "" {
		t.Fatalf("Latest(empty) = %q, %v", latest, err)
	}
	for _, id := range []string{"20260704-100000-aa11", "20260704-110000-bb22"} {
		r, err := New(dir, id, "investigate", "why is it broken")
		if err != nil {
			t.Fatal(err)
		}
		r.Close()
	}
	ids, err := List(dir)
	if err != nil || len(ids) != 2 {
		t.Fatalf("List = %v, %v", ids, err)
	}
	latest, err := Latest(dir)
	if err != nil || latest != "20260704-110000-bb22" {
		t.Fatalf("Latest = %q, %v", latest, err)
	}
}

func TestNewRefusesDuplicateSession(t *testing.T) {
	dir := t.TempDir()
	r, err := New(dir, "dup", "build", "")
	if err != nil {
		t.Fatal(err)
	}
	r.Close()
	if _, err := New(dir, "dup", "build", ""); err == nil {
		t.Fatal("expected error for duplicate session file")
	}
}
