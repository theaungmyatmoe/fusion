# Fusion Roadmap

Long-term product and engineering plan for **Fusion** — a **mobile-friendly, single-binary terminal coding agent**, made by **Fusion AI**.

| | |
|---|---|
| **Status** | Living document |
| **Current line** | Post-monorepo rebuild (product tags `v0.1.x` shipping; internal crate versions may differ) |
| **North star** | Best local coding agent on a **phone and a laptop** — without becoming a heavyweight IDE or mandatory cloud product |

---

## Product pillars (keep these fixed)

| Pillar | Meaning |
|--------|---------|
| **One binary** | Ship `fusion` — no Node runtime required for the agent itself |
| **Mobile-first** | Termux static musl, `--minimal` UI, small screens |
| **Safe by default** | Approvals, sandbox/permissions, explicit YOLO / always-approve |
| **Provider-agnostic** | Cloudflare / xAI / OpenAI-compatible (+ more) behind shared client + config |
| **Local-first** | Config, sessions, skills, memory under `~/.fusion` (or `$FUSION_HOME`) |
| **Fusion identity** | User-facing brand is **Fusion / Fusion AI** — not Grok Build / xAI product chrome |

Everything on this roadmap should strengthen at least one pillar.

---

## Where we are (baseline, 2026-07)

### Done recently (platform shift)

- [x] **Monorepo agent stack** — full TUI, tools, subagents, MCP, shell/leader, update path (`crates/codegen/*`, `crates/common/*`)
- [x] **Product binary** `fusion` (`xai-grok-pager-bin`)
- [x] **White-label identity** — system prompt “Fusion made by Fusion AI”, CLI/UI strings, `~/.fusion` home
- [x] **Static musl** Linux + Termux release targets (`*-unknown-linux-musl`)
- [x] **Installer** maps Termux → `aarch64-unknown-linux-musl` (versioned + legacy asset names)
- [x] **CI** installs `protoc`, builds `-p xai-grok-pager-bin`, focused unit tests
- [x] Default model catalog includes **Cloudflare Workers AI** models

### Known gaps (honest)

- [ ] Internal crate names still `xai-grok-*` (not user-visible, but confuses contributors)
- [ ] Full-workspace `fmt` / `clippy -D warnings` not CI-gated yet
- [ ] Installer vs release naming must stay in lockstep on every tag
- [ ] On-device Termux **from-source** script still outdated vs monorepo layout
- [ ] Docs / user-guide paths partially migrated from grok → fusion

---

## Horizon map

```text
Phase 0  Now → ~6 weeks       Ship v0.2: trust, install, auth, docs
Phase 1  ~2–4 months          Agent depth + UX polish → v0.3
Phase 2  ~4–9 months          Platform + ecosystem → v0.4
Phase 3  ~9–18 months         Integration surface → v0.5 … v1.0
```

---

## Phase 0 — Foundation (now → ~6 weeks)

**Goal:** Make the monorepo rebuild **trustworthy to install and use** as a serious **v0.2**.

### Release & distribution

- [x] Install script understands static musl + versioned assets
- [ ] Tag **v0.2.0** after CI green; verify all four platform tarballs + Termux install path
- [ ] `fusion --version` / release notes match product tag
- [ ] Align `scripts/install.sh` ↔ `.github/workflows/release.yml` asset names in a single source of truth (doc or script comment is not enough — add a smoke test or release checklist)
- [ ] Refresh `scripts/grok-build-termux.sh` / on-device build docs for monorepo + `protoc`

### Config & auth

- [ ] Clear first-run path: `fusion login` or documented env keys → ready to chat
- [ ] Document config precedence: env > project > `~/.fusion/config.toml`
- [ ] Provider readiness in `/status` or equivalent (incomplete credentials → actionable error)
- [ ] `fusion doctor` (config, network, protoc N/A at runtime, ripgrep/git, platform, `$FUSION_HOME`)

### Identity & docs

- [x] Agent prompt identity Fusion / Fusion AI
- [x] README reflects monorepo + install targets
- [ ] User guide under `~/.fusion/docs` fully Fusion-branded (no Grok Build leftovers)
- [ ] Contributor map: “user-facing Fusion vs internal xai-grok-* crate names”

### Reliability

- [ ] CI stays green on `main` (ubuntu + macos build, musl, focused tests)
- [ ] Headless smoke: `fusion -p "echo ok"` against a mock or free-tier path
- [ ] Session crash-safety / resume basic checks

### Phase 0 exit criteria

> New user: install → auth → complete a real coding task on **laptop and Termux** without env archaeology.

---

## Phase 1 — Agent depth (~2–4 months)

**Goal:** Compete on **task completion**, not just UI. Target **v0.3**.

### Planning & execution

- [ ] Stronger plan mode: explore → plan → approve → execute
- [ ] Compaction quality + token accounting users can see
- [ ] Goal / multi-turn harness polish (planner, verifier, summarizer already present — harden)

### Subagents & tools

- [ ] Clear parent–child contracts (handoff, file ownership, failures)
- [ ] Background tasks: list, cancel, attach, surface errors
- [ ] Optional worktree isolation for parallel writes
- [ ] Skills: discover, version, project + user + bundled

### UX

- [ ] Minimal mode parity for mobile (status, providers, approvals)
- [ ] Notification titles / themes fully Fusion-branded (cleanup leftovers)
- [ ] Keyboard/docs accuracy vs real bindings

### Phase 1 exit criteria

> Multi-file feature work with plan + subagents is reliable under real provider rate limits.

---

## Phase 2 — Platform & ecosystem (~4–9 months)

**Goal:** Easy to install and trust on every advertised target. Target **v0.4**.

### Platforms

- [ ] First-class Termux profile (notifications / device skills as opt-in)
- [ ] Alpine/proot “safe shell” docs
- [ ] Binary size / feature flags for constrained devices

### Security

- [ ] Explicit permission tiers: read-only / workspace-write / network / shell
- [ ] Per-tool approval memory
- [ ] Session audit log of shell + writes

### Providers & models

- [ ] Expand catalog + OpenAI-compatible `base_url` hosts
- [ ] Local models (Ollama / llama.cpp OpenAI-compatible)
- [ ] Model routing: cheap explore / strong implement

### Observability

- [ ] Opt-in local telemetry file (latency, tool fails, 429s)
- [ ] Mature `fusion doctor`

### Phase 2 exit criteria

> `fusion doctor` green on macOS + Termux; cold install under one minute.

---

## Phase 3 — Integrations (~9–18 months)

**Goal:** Grow without breaking the single-binary core. Target **v0.5 → v1.0**.

### Headless / CI / gateways

- [ ] Stable `fusion -p` JSON event stream for OpenClaw / CI
- [ ] GitHub Action recipe: “run Fusion on issue”
- [ ] Machine-readable task results for swarm

### IDE-light (optional, late)

- [ ] LSP/editor bridge **only if** it reuses agent core — no forked logic

### Team / multi-repo (optional)

- [ ] Shared skills packs; `AGENTS.md` / Fusion rules convention
- [ ] No mandatory cloud account — stay BYOK / local-first

### Phase 3 exit criteria

> Clear choice for “coding agent on phone + SSH box + laptop,” with a thin integration layer for chat gateways.

---

## Architecture evolution

```text
User-facing
  fusion (binary)     → crates/codegen/xai-grok-pager-bin

Major internal layers (crate names historical)
  pager / TUI         → xai-grok-pager, pager-render, pager-minimal
  shell / session     → xai-grok-shell
  agent / prompts     → xai-grok-agent
  tools               → xai-grok-tools, xai-grok-tools-api
  config / paths      → xai-grok-config  (~/.fusion)
  common              → crates/common/* (protocol, runtime, tracing)
```

### Rules of growth

- New **providers** = shared LLM client + config types, not TUI special cases  
- New **tools** = tools crate + schema + tests  
- New **UX** = events into the pager, not agent drawing UI  
- **White-label**: user strings + paths + prompts stay Fusion; rename internal crates only when it pays off  

### Technical debt (schedule deliberately)

| Debt | When |
|------|------|
| `xai-grok-*` crate rename → `fusion-*` | After v0.2 ship; mechanical, high-churn |
| Tool wire namespace `GrokBuild:*` | Protocol change — needs migration plan |
| Full workspace clippy/fmt in CI | Phase 0–1 once formatting is normalized |
| Giant pager/shell modules | Phase 1 — split by concern |
| Dual config stories (`fusion.toml` vs `~/.fusion/config.toml`) | Phase 0 docs + single recommended path |

---

## Version milestones

| Version | Theme |
|---------|--------|
| **0.1.x** | Early product tags; culminates in monorepo import |
| **0.2** | Install/CI/auth/docs solid; Termux static path proven |
| **0.3** | Plan + subagents + mobile UX reliable |
| **0.4** | Permissions + local models + Termux polish |
| **0.5** | Stable headless JSON / OpenClaw |
| **1.0** | API stability promise, install matrix green, daily-driver bar |

---

## What *not* to build

- Full IDE (VS Code clone)
- Mandatory cloud accounts
- Plugin marketplace before tools + skills are stable
- Multi-provider UI complexity before single-provider reliability
- Shipping `reference/` trees or `target/` in releases
- Dynamic Android NDK builds **unless** static musl fails real devices (static remains default)

---

## How to use this roadmap

1. **One phase theme per month** — don’t mix “rename all crates” + “new provider” + “IDE plugin”.  
2. Every PR: name the **pillar** it serves.  
3. Before features: “does mobile + single binary still win?”  
4. **Phase 0 is not optional** — velocity dies without install/auth/CI trust.

---

## Immediate sequencing

1. Green CI on `main` + tag **v0.2.0** release (verify Termux install)  
2. `fusion doctor` + auth/first-run clarity  
3. Docs/user-guide Fusion-only pass  
4. Plan mode + compaction hardening  
5. Subagent UX + permissions  
6. Headless JSON for OpenClaw  

---

## Related docs

- Root [README](../README.md) — install, config, usage  
- [OpenClaw integration](./openclaw-integration.md)  

---

*Update checkboxes as work lands. Prefer small PRs that close one checkbox over large unscoped rewrites.*
