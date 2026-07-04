# ahab

> "…to the last I grapple with thee." — an agentic harness for Kubernetes systems.

`ahab` drives an LLM agent that **builds** and **maintains** Kubernetes systems while you supervise. The agent writes Kubernetes YAML and Helm templates, applies them through kubectl / helm / flux / argo, and investigates cluster problems down to root cause — but every consequential step (plans, file changes, mutating commands) is shown to you for review before it happens.

## Modes

| Mode | Status | What it does |
|---|---|---|
| **Building** | v1 | PLAN → WRITE → APPLY companion workflow: the agent proposes a plan you approve, writes manifests you review as diffs, then applies them command-by-command with your sign-off and monitors the rollout. |
| **Investigation** | v1 | Point it at a symptom ("pod X is CrashLoopBackOff"); the agent reads cluster state, logs, and linked source repositories, then writes a root-cause report with a suggested fix. |
| **Testing** | planned | Chaos / smoke testing driven by the agent. |
| **Observability** | planned | Provider-agnostic sync with dashboards (Grafana, …). |

## Usage

```sh
ahab build "deploy nginx with 2 replicas and a service"
ahab investigate "payments pod is CrashLoopBackOff"
ahab history [--session <id>]
```

Every command the agent runs is recorded to `.ahab/history/<session>.jsonl`; `ahab history` renders the session's command tree. Investigation reports land in `.ahab/reports/`.

## Configuration

`~/.config/ahab/config.yaml` (user) merged with `./.ahab/config.yaml` (project):

```yaml
provider: anthropic        # LLM provider
model: claude-opus-4-8
kubeContext: kind-dev
namespace: default
repos:                     # source repos the agent may explore during investigations
  - ../payments-service
```

Authentication uses `ANTHROPIC_API_KEY` (or an `ant auth login` profile).

## Safety model

- The agent's shell access is limited to allowlisted `kubectl` / `helm` / `flux` / `argo` invocations; everything else is rejected before execution.
- Mutating commands always require interactive approval (deny with feedback and the agent adjusts). `--auto` mode is reserved for a future release.
- Reads (get/describe/logs/events) are auto-approved but still recorded.

## Development

```sh
go build ./... && go test ./...
```
