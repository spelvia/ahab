// Package executor runs allowlisted kubectl / helm / flux / argocd commands.
// Commands are exec'd directly (no shell), validated against per-binary verb
// allowlists, and kept human-readable so recorded history matches exactly
// what an operator would type.
package executor

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"slices"
	"strings"
	"time"
)

// ErrNoPreview is returned by Preview when the command has no dry-run form.
var ErrNoPreview = errors.New("no preview available for this command")

// defaultTimeout bounds a single command execution.
const defaultTimeout = 2 * time.Minute

// Command is one requested invocation: which executor and its arguments
// (excluding the binary itself and any injected context/namespace flags).
type Command struct {
	Executor string   `json:"executor"`
	Args     []string `json:"args"`
}

// Result is the outcome of a Run.
type Result struct {
	Output   string
	ExitCode int
}

// spec describes one supported binary.
type spec struct {
	bin        string
	readVerbs  []string
	writeVerbs []string
	// contextFlag injects the kube context ("" = unsupported).
	contextFlag string
	// namespaceFlag injects the default namespace ("" = unsupported).
	namespaceFlag string
	// denyFlags are rejected wherever they appear (interactive/hanging modes).
	denyFlags []string
	// dryRun transforms args into a preview invocation for the given verb;
	// returns nil when the verb has no dry-run form.
	dryRun func(verb string, args []string) []string
}

var specs = map[string]*spec{
	"kubectl": {
		bin:           "kubectl",
		readVerbs:     []string{"get", "describe", "logs", "events", "top", "explain", "api-resources", "api-versions", "version", "cluster-info", "diff"},
		writeVerbs:    []string{"apply", "create", "delete", "patch", "scale", "rollout", "annotate", "label", "set", "expose", "wait"},
		contextFlag:   "--context",
		namespaceFlag: "--namespace",
		denyFlags:     []string{"--watch", "-w", "--follow", "--edit"},
		dryRun: func(verb string, args []string) []string {
			switch verb {
			case "apply", "create", "delete", "patch", "scale", "annotate", "label", "set", "expose":
				return append(slices.Clone(args), "--dry-run=server")
			}
			return nil
		},
	},
	"helm": {
		bin:           "helm",
		readVerbs:     []string{"list", "status", "get", "history", "show", "search", "template", "lint", "env", "version", "dependency", "repo"},
		writeVerbs:    []string{"install", "upgrade", "uninstall", "rollback"},
		contextFlag:   "--kube-context",
		namespaceFlag: "--namespace",
		denyFlags:     []string{"--wait-for-jobs"},
		dryRun: func(verb string, args []string) []string {
			switch verb {
			case "install", "upgrade", "uninstall", "rollback":
				return append(slices.Clone(args), "--dry-run")
			}
			return nil
		},
	},
	"flux": {
		bin:           "flux",
		readVerbs:     []string{"get", "check", "export", "stats", "tree", "trace", "events", "version"},
		writeVerbs:    []string{"reconcile", "suspend", "resume", "create", "delete", "push"},
		contextFlag:   "--context",
		namespaceFlag: "--namespace",
		denyFlags:     []string{"--watch"},
		dryRun: func(verb string, args []string) []string {
			switch verb {
			case "create", "delete", "push":
				return append(slices.Clone(args), "--export")
			}
			return nil
		},
	},
	"argocd": {
		bin:        "argocd",
		readVerbs:  []string{"app", "proj", "cluster", "repo", "version", "account"},
		writeVerbs: []string{}, // argocd verbs are nested; classified below
		denyFlags:  []string{"--web"},
		dryRun: func(verb string, args []string) []string {
			if verb == "app" && len(args) >= 2 && args[1] == "sync" {
				return append(slices.Clone(args), "--dry-run")
			}
			return nil
		},
	},
}

// argocd nests its mutations under "app <subverb>"; these subverbs mutate.
var argocdWriteSubverbs = []string{"sync", "create", "delete", "rollback", "set", "unset", "patch", "edit", "actions", "terminate-op"}

// Runner validates and executes commands with cluster context injection.
type Runner struct {
	kubeContext string
	namespace   string
	timeout     time.Duration
}

func New(kubeContext, namespace string) *Runner {
	return &Runner{kubeContext: kubeContext, namespace: namespace, timeout: defaultTimeout}
}

// Executors lists the supported executor names.
func Executors() []string {
	names := make([]string, 0, len(specs))
	for name := range specs {
		names = append(names, name)
	}
	slices.Sort(names)
	return names
}

// Validate rejects commands outside the allowlist. It does not consult the
// cluster; it is a pure argument check.
func (r *Runner) Validate(cmd Command) error {
	s, ok := specs[cmd.Executor]
	if !ok {
		return fmt.Errorf("unsupported executor %q (supported: %s)", cmd.Executor, strings.Join(Executors(), ", "))
	}
	if len(cmd.Args) == 0 {
		return errors.New("empty command")
	}
	verb := cmd.Args[0]
	if !slices.Contains(s.readVerbs, verb) && !slices.Contains(s.writeVerbs, verb) {
		return fmt.Errorf("%s %s is not allowed (allowed verbs: %s)",
			cmd.Executor, verb, strings.Join(append(slices.Clone(s.readVerbs), s.writeVerbs...), ", "))
	}
	for _, arg := range cmd.Args {
		if slices.Contains(s.denyFlags, arg) {
			return fmt.Errorf("flag %s is not allowed (interactive/blocking mode)", arg)
		}
		if strings.ContainsAny(arg, "\x00\n") {
			return fmt.Errorf("argument contains forbidden control characters")
		}
	}
	return nil
}

// IsRead reports whether the command only reads cluster state.
func (r *Runner) IsRead(cmd Command) bool {
	s, ok := specs[cmd.Executor]
	if !ok || len(cmd.Args) == 0 {
		return false
	}
	verb := cmd.Args[0]
	if cmd.Executor == "argocd" {
		if verb == "app" && len(cmd.Args) >= 2 && slices.Contains(argocdWriteSubverbs, cmd.Args[1]) {
			return false
		}
		return slices.Contains(s.readVerbs, verb)
	}
	return slices.Contains(s.readVerbs, verb)
}

// CommandLine returns the exact argv that Run would execute, including
// injected context/namespace flags — for approval prompts and history.
func (r *Runner) CommandLine(cmd Command) []string {
	s := specs[cmd.Executor]
	argv := []string{s.bin}
	argv = append(argv, cmd.Args...)
	if r.kubeContext != "" && s.contextFlag != "" && !hasFlag(cmd.Args, s.contextFlag) {
		argv = append(argv, s.contextFlag, r.kubeContext)
	}
	if r.namespace != "" && s.namespaceFlag != "" && !hasFlag(cmd.Args, s.namespaceFlag) && !hasFlag(cmd.Args, "-n") && !hasFlag(cmd.Args, "-A") && !hasFlag(cmd.Args, "--all-namespaces") {
		argv = append(argv, s.namespaceFlag, r.namespace)
	}
	return argv
}

// Preview runs the dry-run form of a mutating command, if one exists.
func (r *Runner) Preview(ctx context.Context, cmd Command) (string, error) {
	if err := r.Validate(cmd); err != nil {
		return "", err
	}
	s := specs[cmd.Executor]
	preview := s.dryRun(cmd.Args[0], cmd.Args)
	if preview == nil {
		return "", ErrNoPreview
	}
	res, err := r.Run(ctx, Command{Executor: cmd.Executor, Args: preview})
	return res.Output, err
}

// Run validates and executes the command, returning combined output. A
// non-zero exit is reported in Result.ExitCode with a nil error; err is
// reserved for validation and process-start failures.
func (r *Runner) Run(ctx context.Context, cmd Command) (Result, error) {
	if err := r.Validate(cmd); err != nil {
		return Result{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()

	argv := r.CommandLine(cmd)
	proc := exec.CommandContext(ctx, argv[0], argv[1:]...)
	out, err := proc.CombinedOutput()
	res := Result{Output: string(out)}
	var exitErr *exec.ExitError
	switch {
	case err == nil:
	case errors.As(err, &exitErr):
		res.ExitCode = exitErr.ExitCode()
	default:
		return res, fmt.Errorf("running %s: %w", strings.Join(argv, " "), err)
	}
	return res, nil
}

func hasFlag(args []string, flag string) bool {
	for _, a := range args {
		if a == flag || strings.HasPrefix(a, flag+"=") {
			return true
		}
	}
	return false
}
