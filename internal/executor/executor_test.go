package executor

import (
	"slices"
	"strings"
	"testing"
)

func TestValidate(t *testing.T) {
	r := New("", "")
	cases := []struct {
		name    string
		cmd     Command
		wantErr string // "" = valid
	}{
		{"kubectl get", Command{"kubectl", []string{"get", "pods"}}, ""},
		{"kubectl apply", Command{"kubectl", []string{"apply", "-f", "deploy.yaml"}}, ""},
		{"kubectl exec denied", Command{"kubectl", []string{"exec", "-it", "pod", "--", "sh"}}, "not allowed"},
		{"kubectl port-forward denied", Command{"kubectl", []string{"port-forward", "svc/x", "8080:80"}}, "not allowed"},
		{"kubectl watch flag denied", Command{"kubectl", []string{"get", "pods", "--watch"}}, "not allowed"},
		{"kubectl follow denied", Command{"kubectl", []string{"logs", "pod-x", "--follow"}}, "not allowed"},
		{"helm install", Command{"helm", []string{"install", "nginx", "bitnami/nginx"}}, ""},
		{"helm plugin denied", Command{"helm", []string{"plugin", "install", "evil"}}, "not allowed"},
		{"flux reconcile", Command{"flux", []string{"reconcile", "kustomization", "apps"}}, ""},
		{"flux bootstrap denied", Command{"flux", []string{"bootstrap", "github"}}, "not allowed"},
		{"argocd app sync", Command{"argocd", []string{"app", "sync", "myapp"}}, ""},
		{"argocd login denied", Command{"argocd", []string{"login", "argocd.example.com"}}, "not allowed"},
		{"unknown executor", Command{"bash", []string{"-c", "echo hi"}}, "unsupported executor"},
		{"empty args", Command{"kubectl", nil}, "empty command"},
		{"control chars", Command{"kubectl", []string{"get", "pods\nsecrets"}}, "control characters"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := r.Validate(tc.cmd)
			if tc.wantErr == "" {
				if err != nil {
					t.Fatalf("Validate(%v) = %v, want nil", tc.cmd, err)
				}
				return
			}
			if err == nil || !strings.Contains(err.Error(), tc.wantErr) {
				t.Fatalf("Validate(%v) = %v, want error containing %q", tc.cmd, err, tc.wantErr)
			}
		})
	}
}

func TestIsRead(t *testing.T) {
	r := New("", "")
	reads := []Command{
		{"kubectl", []string{"get", "pods"}},
		{"kubectl", []string{"logs", "pod-x"}},
		{"helm", []string{"list"}},
		{"flux", []string{"get", "kustomizations"}},
		{"argocd", []string{"app", "list"}},
		{"argocd", []string{"app", "get", "myapp"}},
	}
	writes := []Command{
		{"kubectl", []string{"apply", "-f", "x.yaml"}},
		{"kubectl", []string{"delete", "pod", "x"}},
		{"helm", []string{"install", "x", "chart"}},
		{"flux", []string{"reconcile", "source", "git", "x"}},
		{"argocd", []string{"app", "sync", "myapp"}},
		{"argocd", []string{"app", "delete", "myapp"}},
	}
	for _, c := range reads {
		if !r.IsRead(c) {
			t.Errorf("IsRead(%v) = false, want true", c)
		}
	}
	for _, c := range writes {
		if r.IsRead(c) {
			t.Errorf("IsRead(%v) = true, want false", c)
		}
	}
}

func TestCommandLineInjection(t *testing.T) {
	r := New("kind-dev", "payments")

	got := r.CommandLine(Command{"kubectl", []string{"get", "pods"}})
	want := []string{"kubectl", "get", "pods", "--context", "kind-dev", "--namespace", "payments"}
	if !slices.Equal(got, want) {
		t.Errorf("CommandLine = %v, want %v", got, want)
	}

	// Explicit flags must not be duplicated.
	got = r.CommandLine(Command{"kubectl", []string{"get", "pods", "--namespace", "other", "--context", "prod"}})
	if count(got, "--namespace") != 1 || count(got, "--context") != 1 {
		t.Errorf("flags duplicated: %v", got)
	}

	// -A suppresses namespace injection.
	got = r.CommandLine(Command{"kubectl", []string{"get", "pods", "-A"}})
	if slices.Contains(got, "--namespace") {
		t.Errorf("namespace injected alongside -A: %v", got)
	}

	// helm uses --kube-context.
	got = r.CommandLine(Command{"helm", []string{"list"}})
	if !slices.Contains(got, "--kube-context") {
		t.Errorf("helm context flag missing: %v", got)
	}

	// argocd gets no kube context.
	got = r.CommandLine(Command{"argocd", []string{"app", "list"}})
	if slices.Contains(got, "--context") {
		t.Errorf("argocd should not get --context: %v", got)
	}
}

func count(args []string, flag string) int {
	n := 0
	for _, a := range args {
		if a == flag {
			n++
		}
	}
	return n
}
