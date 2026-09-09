# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Palantir is a local-only desktop app (Tauri 2 + Rust backend, React/TypeScript frontend) for managing Kanban tickets executed by local code agents — Codex and Claude are both wired in and selectable per ticket. Each ticket runs on a dedicated Git branch and lands in a Review column — the app never merges, pushes, deletes branches, or configures Git credentials itself.

Preferences always live in the local SQLite database under the Tauri app data dir — but `preferences.executionProfile` can be `"local"` (all state — tickets, repositories, execution queue, sessions/logs, plans — in that same local SQLite) or `"remote"` (the board instead operates against a separate, independent headless server, `app/server/`, binary `palantir-server`, over HTTP — its own SQLite, its own instance, not synced with the local one). See "Local vs. remote dispatch" below for how the frontend picks which backend to talk to.

## Commands

All commands run from `app/`:

```bash
npm install          # install frontend deps
npm run tauri dev    # run the full desktop app (Tauri + Vite) in dev mode
npm run dev           # frontend only (Vite dev server, no Tauri shell)
npm run build         # tsc typecheck + vite build (frontend)
npm run tauri build   # full production bundle
```

Rust backend (from `app/src-tauri/`):

```bash
cargo test                    # run all backend tests
cargo test migration_keeps_snapshots   # run a single test by name
cargo build
```

There is no separate lint script; `npm run build` (tsc) is the frontend's correctness gate, `cargo test` is the backend's.

## Architecture

### Backend: a single-file command surface

Nearly all backend logic lives in `app/src-tauri/src/lib.rs` (~1900 lines, one file by design so far — do not assume there are other Rust modules). `main.rs` just calls `palantir_lib::run()`. Structure inside `lib.rs`:

- **State**: `AppState(Mutex<Connection>)` wraps a single SQLite connection, managed by Tauri and accessed via `tauri::State` in every command.
- **Migration**: `migrate()` runs on startup (idempotent `CREATE TABLE IF NOT EXISTS` style) and also backfills/repairs data — e.g. recovering from an app crash mid-execution (see `run()`'s `setup` closure, which marks stale `running` executions as `interrupted`, resets their tickets to `todo`, requeues them with `retry_required`, and trims `execution_logs` to the last 2000 rows).
- **Agent adapters**: the `AgentAdapter` trait (`cli_name`, `is_available`, `start`, `resume`, `session_id_from_output`) abstracts over CLI-based coding agents. `CodexAdapter` (`codex exec`) and `ClaudeAdapter` (`claude --print`) are both implemented and wired into `adapter_for()`; `default_preferences()` enables both agents and their model lists (`CODEX_MODELS`/`CLAUDE_MODELS`) out of the box. Adding a new agent means implementing this trait and extending `adapter_for()` plus the `enabled_agents`/`allowed_models` preference validation (and the matching TS lists in `App.tsx`: `modelsByAgent`, `agentLabels`, `allModels`).
- **Tauri commands**: every `#[tauri::command]` fn is registered explicitly in the `invoke_handler!` list at the bottom of `run()` — a new command must be added there or the frontend's `invoke()` call will fail silently at runtime (not a compile error).
- **Execution flow**: `move_ticket` (backlog→todo) validates the workspace is a Git repo, snapshots the base commit, creates/reuses a `palantir/...` branch (`branch_name()`), and enqueues; `schedule_queued`/`reserve_next_queued` implement a FIFO queue bounded by `preferences.max_concurrent_tasks`; `start_agent`/`start_agent_with_context` spawn the CLI process and stream output into `execution_logs`; `detect_changes`/`execution_summary`/`terminal_state` parse the finished process into a Session record surfaced in Review. Ticket status transitions are restricted by `valid_transition()` — the frontend never gets to move a ticket to an arbitrary column.
- **Planning tickets**: `kind: "planning"` tickets produce a `Plan` (`parse_plan` extracts structured items from agent output); `approve_plan` materializes each item as a child ticket (`parent_id` set) in an isolated sub-kanban, without adding them to the main board.
- **Repositories**: a separate catalog (`repositories` table) that tickets reference by `repository_id`; `repository_path()` resolves a ticket's working directory from that reference, with unique-path validation in `valid_repository()`.
- Tests live inline in `#[cfg(test)] mod tests` at the end of `lib.rs` — follow that convention (no separate test files/crates) when adding backend tests.

### Frontend: one component, explicit backend contract

`app/src/App.tsx` is the entire UI (a single functional component with local `useState`, no router, no component library). It polls every 2s (`setInterval(load, 2000)`) rather than using push events — the UI is a thin, eventually-consistent view over whichever backend is active. Since `load()` is captured once by that `useEffect(..., [])`, anything it reads that can change later (`preferences`, `settingsOpen`) must go through a ref synced each render (`preferencesRef`, `settingsOpenRef`), not the raw state — a plain closure over that state would go stale and never see updates.

**Local vs. remote dispatch**: most `invoke("command_name", { ... })` calls go straight to a `#[tauri::command]` in `lib.rs`. But `preferences.executionProfile` can be `"remote"`, and the small set of `api*` wrapper functions above `App`'s definition (`apiListTickets`, `apiCreateTicket`, `apiMoveTicket`, `apiListRepositories`, etc.) branch on `isRemoteProfile(prefs)`: local mode calls the Tauri command as before, remote mode calls `remote_request` (a generic HTTP proxy `#[tauri::command]` in `lib.rs`) which forwards to the configured `palantir-server` endpoint over `ureq`. This proxy exists specifically because a direct frontend `fetch()` to that endpoint would be blocked by the Tauri webview's CORS enforcement (`palantir-server` sends no CORS headers) — native Rust HTTP requests aren't subject to that. Call sites that use these `api*` wrappers must pass `preferences` explicitly (never assume local). Operations `palantir-server` doesn't implement yet (planning/sub-kanban, archiving, retry, stop, follow-up) short-circuit with `REMOTE_UNSUPPORTED` when remote, rather than trying and failing.

Payload/return types are hand-duplicated as TS types in `App.tsx` (there is no shared schema/codegen — when changing a Rust command's shape, update the matching TS type by hand, and if it's an operation the remote server also implements, update `app/server/src/main.rs`'s matching handler too — the two aren't shared and can drift). `palantir-server`'s ticket/session shapes lack `kind`/`archived`/`effectiveConfig` (it doesn't support those concepts) — `toLocalTicket`/`toLocalSession` fill sane defaults so the rest of the UI doesn't need remote-aware branches.

CSS is split by concern and all imported into `App.tsx` (`palantir-theme.css` for tokens, `app-functions.css`, `ticket-modal.css`, `modern-desktop.css`, `ticket-detail-modal.css`, `compact-desktop.css`, `repositories.css`, plus `App.css`).

**Dead code note**: `app/src/review-details.ts`, `session-panel.ts`, and `transition-guard.ts` are not imported anywhere (not from `main.tsx`, `App.tsx`, or `index.html`) and query DOM structures (`.review .transitions`, `.rail button:nth-of-type(2)`) that no longer match current markup — they predate the current single-component `App.tsx` and are inert. Don't assume they run; don't build on them without first checking whether they should be deleted instead.

### Headless server (`app/server/`)

An independent, standalone Rust binary crate (`palantir-server`) — **not** a library dependency of `src-tauri` and not extracted from it; the small pieces that are genuinely reusable (the `AgentAdapter` trait, `CodexAdapter`/`ClaudeAdapter`, git helpers, execution-summary/branch-naming logic) are deliberately duplicated rather than shared, to avoid an invasive refactor of the tested desktop code. It has its own SQLite schema (a subset: repositories, tickets, executions, execution_queue, preferences — no planning/sub-kanban/archiving), its own `migrate()`, and exposes the same kind of operations as the desktop's Tauri commands as a plain HTTP+JSON API (`tiny_http`, synchronous, thread-per-request) with bearer-token auth on every route except `GET /health`. Queue scheduling (`reserve_next_queued`/`schedule_queued`) and the spawn-CLI-then-background-thread-writes-back execution pattern mirror `lib.rs`'s approach closely enough that changes to one likely want the equivalent change in the other. Tests live the same way, in `#[cfg(test)] mod tests` at the end of `main.rs`.

### Spec-driven development (OpenSpec)

This repo tracks features as OpenSpec changes under `openspec/`. `openspec/specs/<capability>/spec.md` holds the current accepted behavior per capability (`kanban-ticket-management`, `local-agent-execution`, `local-workspace-persistence`, `review-session-follow-up`, `sub-kanban-planning`, `execution-queue`, `application-preferences`). `openspec/changes/` holds in-flight or archived change proposals, each with `proposal.md`, `design.md`, `tasks.md`, and a spec delta under `specs/<capability>/spec.md`; completed changes move to `openspec/changes/archive/`. `.agents/skills/openspec-*` define the propose → apply → archive workflow (scaffold artifacts, then implement tasks, then archive) — check `openspec/changes/` for an in-progress change and its `tasks.md` checklist before starting unrelated work, since a change may already be mid-implementation.
