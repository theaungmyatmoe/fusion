# Fusion Roadmap

Long-term product and engineering plan for Fusion — a **mobile-first, single-binary terminal coding agent**.

| | |
|---|---|
| **Status** | Living document |
| **Current version** | 0.1.x |
| **North star** | Best local coding agent that runs well on a phone **and** a laptop — without becoming a heavyweight IDE or cloud product |

---

## Product pillars (keep these fixed)

| Pillar | Meaning |
|--------|---------|
| **One binary** | No Node runtime required for the agent itself |
| **Mobile-first** | Termux, small screens, simple REPL fallback |
| **Safe by default** | Plan mode, path sandbox, explicit YOLO |
| **Provider-agnostic core** | Cloudflare / xAI / OpenAI (+ more later) behind one `Config` + `LlmClient` |
| **Local-first** | Sessions, skills, taste/design on disk |

Everything on this roadmap should strengthen at least one pillar.

---

## Horizon map

```text
Phase 0  Now → ~6 weeks       Foundation quality
Phase 1  ~2–4 months          Agent depth + UX polish
Phase 2  ~4–9 months          Platform + ecosystem
Phase 3  ~9–18 months         Product surface expansion
```

---

## Phase 0 — Foundation (now → ~6 weeks)

**Goal:** Make what exists trustworthy and shippable as a serious **v0.2**.

### Config & providers

- [ ] Provider auth model in **core** (`ProviderAuth`, completeness checks) — not only TUI
- [ ] Structured TOML load/save (less string surgery)
- [ ] First-run readiness: block chat or force `/providers` when incomplete
- [ ] Document precedence: env > project > global; placeholders = unset
- [x] Ignore template placeholders (`YOUR_*`) when loading credentials
- [x] Cloudflare 2-step credential wizard (API token → account ID)
- [x] Create `~/.config/fusion/fusion.toml` when saving credentials if missing
- [x] Provider-aware API key selection (xAI / OpenAI keys from file, not only env)

### Reliability

- [ ] Golden tests for config merge, save-create-toml, CF dual-credential
- [x] Unit tests: placeholders, create-toml-when-missing, xAI from file
- [ ] LLM client: clearer auth vs rate-limit vs model-not-found errors
- [ ] Session resume edge cases; crash-safe session write

### Agent quality bar

- [ ] Tool call robustness (stream fragments, fallback parsers) regression tests
- [ ] Shell/path sandbox audit — formalize policy (safe path resolution already exists)
- [ ] `/status` shows provider readiness, rate-limit pressure, session path

### Release hygiene

- [ ] Version/changelog discipline; install script + CI matrix (macOS, Linux musl, Android)
- [ ] Fix repo URL consistency (`Cargo.toml` still references old `zencode` path)

### Phase 0 exit criteria

> New user can install → 2-step Cloudflare setup → complete a real coding task without env hacks.

---

## Phase 1 — Agent depth (~2–4 months)

**Goal:** Compete with Codex/OpenCode on **task completion**, not just UI. Target **v0.3**.

### Planning & execution

- [ ] Stronger **plan mode**: explore → plan file → approve → execute
- [ ] Grill/design interview as a real phase with structured outputs
- [ ] Compact history (`/compact`) with quality guarantees + token accounting

### Swarm / sub-agents

- [ ] Personas as first-class: better prompts, tool subsets, cost models
- [ ] Parent–child contracts: clearer handoff, file ownership, merge conflicts
- [ ] Background tasks UI: list, cancel, attach, surface failures cleanly
- [ ] Optional worktree isolation for parallel writes

### Tools

- [ ] Patch quality (`apply_patch` / `search_replace`) as the main edit path
- [ ] LSP-lite or tree-sitter symbols (deepen `get_symbols`)
- [ ] Browser/debug tool hardening or make it optional/feature-gated
- [ ] Skills system: discover, version, share; global + project skills

### Taste & design memory

- [ ] Close the loop on `taste` / `design` scanners: inject into system prompt consistently, refresh on demand, show what was learned

### Phase 1 exit criteria

> Multi-file feature work with swarm + plan mode is reliable under Cloudflare rate limits.

---

## Phase 2 — Platform & distribution (~4–9 months)

**Goal:** Fusion is easy to install and trust on every target we advertise. Target **v0.4**.

### Platforms

- [ ] First-class Termux profile (API skill, notifications, camera/location as opt-in skills)
- [ ] Alpine/proot sandbox docs + “safe shell” profile
- [ ] iSH: always simple REPL; reduce binary size / feature flags

### Security model

- [ ] Explicit permission tiers: read-only / workspace-write / network / shell
- [ ] Per-tool approval memory (“always allow `read_file` in this repo”)
- [ ] Audit log of shell + writes for a session

### Providers & models

- [ ] Expand catalog (more CF models, OpenAI-compatible `base_url` hosts)
- [ ] Local models via Ollama / llama.cpp OpenAI-compatible endpoint
- [ ] Model routing: cheap for explore, strong for implement (arbitrage matured)

### Observability

- [ ] Optional local telemetry file (opt-in): latency, tool fail rates, 429s
- [ ] `fusion doctor` — config, network, token, ripgrep, git, platform

### Phase 2 exit criteria

> `fusion doctor` green on macOS + Termux; install under one minute.

---

## Phase 3 — Ecosystem & product surface (~9–18 months)

**Goal:** Grow without breaking the single-binary core. Target **v0.5 → v1.0**.

### Headless / integration

- [ ] Stable CLI for OpenClaw / CI: `fusion -p` with JSON event stream
- [ ] Machine-readable task results for swarm (task sessions already exist — expose API)
- [ ] GitHub Action or simple CI recipe: “run Fusion on issue”

### IDE-light (optional, late)

- [ ] LSP server or editor bridge **only if** it reuses agent core (do not fork logic into plugins)
- [ ] Or: stay terminal-only and double down on mobile + SSH workflows

### Multi-repo / team (optional)

- [ ] Shared skills packs; project `AGENTS.md` / fusion rules convention
- [ ] No cloud account required — stay local-first

### Monetization-compatible (if ever)

- [ ] “Bring your own key” forever
- [ ] Optional hosted Cloudflare Worker template that frontends Workers AI (docs, not mandatory)

### Phase 3 exit criteria

> Fusion is a clear choice for “coding agent on phone + SSH box + laptop,” with a thin integration layer for chat gateways.

---

## Architecture evolution

Keep the workspace; grow responsibilities carefully:

```text
fusion-cli     → entry, flags, doctor, upgrade
fusion-tui     → presentation only (thin over agent events)
fusion-agent   → loop, tools, swarm, personas
fusion-llm     → transport, retries, streaming
fusion-core    → config, models, session, policies, paths
```

### Rules of growth

- New providers = `fusion-llm` + `fusion-core` auth types, **not** TUI special cases
- New tools = `fusion-agent/tools` + schema + tests
- New UX = events on `AgentEvent`, not agent calling Ratatui
- Avoid a sixth crate until a boundary is painful (e.g. `fusion-tools` if tools explode)

### Technical debt to schedule deliberately

| Debt | When to pay |
|------|-------------|
| Giant `app.rs` / `ui.rs` | Phase 1 — split by mode (chat, setup, swarm panel) |
| String TOML credential edit | Phase 0–1 |
| Flat `Config.api_key` | Phase 0 |
| Reference copies under `reference/` | Keep out of release binary; document “inspiration only” |
| Simple REPL feature parity | Phase 1–2 (at least `/providers`, `/status`, readiness) |

---

## Version milestones

| Version | Theme |
|---------|--------|
| **0.2** | Config/auth solid, first-run, doctor, tests, docs |
| **0.3** | Plan mode + compact + swarm UX complete |
| **0.4** | Permissions model + local models + Termux polish |
| **0.5** | Stable headless JSON / OpenClaw integration |
| **1.0** | API stability promise, install matrix green, “daily driver” bar |

---

## What *not* to build

- Full IDE (VS Code clone)
- Mandatory cloud accounts
- Plugin marketplace before tools + skills are stable
- Multi-provider chat UI complexity before single-provider reliability
- Embedding the huge `reference/` trees into the product

---

## How to use this roadmap

1. **Pick one phase theme per month** — do not mix “new provider” + “IDE plugin” + “swarm rewrite.”
2. Every PR: name the **pillar** it serves.
3. Before features: ask “does mobile + single binary still win?”
4. **Phase 0 is not optional** — long-term velocity dies without config/auth/tests.

---

## Immediate sequencing

1. Phase 0 auth model + readiness + tests (finish foundation already started)
2. `fusion doctor` + better errors
3. Plan mode + compact hardening
4. Swarm UX + permissions
5. Headless JSON for OpenClaw

---

## Related docs

- [OpenClaw integration](./openclaw-integration.md)
- Root [README](../README.md) — install, config, usage

---

*Update checkboxes as work lands. Prefer small PRs that close one checkbox over large unscoped rewrites.*
