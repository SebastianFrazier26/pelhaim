# pelhaim: design

Date: 2026-11-23. Status: **approved by the owner on 2026-11-23 with three changes** (§5.2), after four earlier rulings the same day (§5.1). No product code exists yet.

## How to read this doc

Every design statement carries one of three tags:

- **[Owner]**: the owner decided this. A builder does not reopen it without asking.
- **[Agent's call]**: the design agent picked this and gave a reason. It is a default. The owner should confirm or overturn it, and the builder may push back with evidence.
- **[Open]**: deliberately left to the builder. The doc gives a default where it has one.

Facts about other tools are marked the same way as in the visual spec:

- "verified 2026-11-23" means I checked it today on this machine, against `claude --help` (Claude Code 2.1.331), `codex exec --help` (codex-cli 0.158.0), or files on disk;
- "unverified" means it comes from memory or inference, and the builder must check it before relying on it.

Inputs read for this doc:

- `~/Developer/pelhaim-reference/VISUAL-SPEC.md` and its images. These are private reference: never commit them, and never copy them into this repo.
- The look spike `~/Developer/pelhaim-spike/src/main.rs` (macroquad, throwaway).
- The loosy-goose session parsers, `~/Developer/loosy-goose/src/loosy_goose/sessions/{claude,codex,model}.py`.
- `~/Developer/DECISION-LOG.md`, for its format.

---

## 1. Intent and why

pelhaim is a model-agnostic desktop app, launched from the CLI, for **running and watching AI agents**. It draws them as a subway control-room track board. **[Owner]**

The look comes from the control-room wall in *The Taking of Pelham 123* (2009). The film is inspiration only. pelhaim ships no film assets, names or logos. **[Owner]**

Why it should exist:

- The owner runs about 7 projects at once through one Claude Code session that acts as an orchestrator. That session routes agent questions to the owner as modals and keeps a cross-project decision log (`~/Developer/DECISION-LOG.md`).
- That works, but it lives in one scrolling terminal. You can't see at a glance which agents are working, which are waiting on you, which failed and which are done, across every project.
- A track board answers exactly that question at a glance. That is what a train board is for.

pelhaim has to do that orchestrator's job day to day:

- show every agent across every project;
- surface questions as alerts you can answer in one click;
- record decisions;
- in time, plan and dispatch the work itself. **[Owner]**

## 2. Goals, non-goals, anti-goals

### Goals

1. **One board for all agent work.** It covers every project, every session and every subagent, with state readable from across the room. **[Owner]**
2. **Three modes, one look:** Observe, Launch and Direct. The board looks and feels identical in all three. **[Owner]**
3. **Questions never get lost.**
   - Every question, notification and result becomes an alert popup.
   - An alert is quick to answer (click, or press a number key) and quick to dismiss (✕ or Esc).
   - A dismissed alert goes to a tray. It is not deleted. **[Owner]** for the popups; **[Agent's call]** for the tray.
4. **Planning first.**
   - The dispatcher plans before it acts. Plan steps become stations, and dependencies become edges.
   - The owner sees the plan, and edits or approves it, before any train departs. **[Owner]**
5. **The decision log is a first-class output.** Answers and approvals can be logged, in the existing `DECISION-LOG.md` format, with one click. **[Owner]** for the intent; **[Agent's call]** for the mechanism (§7.6).
6. **Model-agnostic.** It works with Claude Code, Codex, Anthropic's API, and local open-weight models behind OpenAI-compatible servers (Ollama, LM Studio, MLX server). **[Owner]**

### Non-goals (v1)

These are not in v1. Each could come later.

- A terminal emulator. "Take over a train" opens a real terminal; it does not embed one. **[Agent's call]**
- An IDE, diff viewer or code browser. The board links out to these.
- Multi-user use or a web UI. pelhaim is local and single-owner.
  - Remote control from the owner's own iPhone is planned for M5 (§8.7). It is the only remote surface.
- A replacement for GitHub Issues, PRs or Actions. Lines map to repos; they do not mirror the tracker.
- Windows support in v1. See [Open] Q1.
- Cost accounting beyond showing token and cost counts that a session already reports.

### Anti-goals (never)

- **Not a toy or screensaver.** Every pixel encodes real state. Nothing decorative moves unless something real changed. **[Owner]**
- **Not a 1:1 copy of the film.**
  - No frames, traced artwork, names ("Pelham", "MTA" and so on) or logos.
  - The font is drawn fresh: inspired by the lettering, not traced from it.
  - Colours are measured values that pelhaim then deliberately mutes. **[Owner]**
- **Never silently acts on an observed session.** Observe mode is read-only. pelhaim never writes to `~/.claude` or `~/.codex`, never types into another process, and never answers on the session's behalf. **[Owner]**
- **Never commits to or merges into `main`, and never pushes or opens a PR without an owner-approved alert.** **[Owner], 2026-11-23** (§7.8).
- **Never auto-approves a tool call** in Direct or Launch mode without an explicit policy the owner set. **[Agent's call]**, from the owner's security norms.
- **Never ships transcript content off the machine.** There is no telemetry and no crash upload. Provider calls carry only what the agent itself sends. **[Agent's call]**
- **No LLM call standing in for the core.** Planning-graph handling, scheduling, state derivation and layout are written code. An LLM call is legitimate only as the agent being orchestrated, or as the planner's author of a draft plan. This follows the owner's "build from scratch" norm.

## 3. Constraints

- **Language: Rust.** Rendering is in a native GPU window launched from the CLI. **[Owner]**
- **Platform:** macOS first. This machine runs macOS 26.5.1. **[Agent's call]**
  - Linux must also build and run, because CI runs there and the portable wgpu path costs nothing extra.
- **Toolchain:** Rust 1.100.0 is installed in `~/.cargo/bin` (verified 2026-11-23). It is **not on the PATH of non-interactive shells** on this machine, so agents must call `~/.cargo/bin/cargo` or fix PATH.
  - Use edition 2024, like the spike.
  - Pin the toolchain in `rust-toolchain.toml`.
- **Repo norms (owner's global norms):**
  - public by default;
  - README for a stranger, MIT `LICENSE`, per-repo `CLAUDE.md` with build, test and run commands, and `.env.example` with dummy values;
  - CI in the first code commit;
  - `CHANGELOG.md` for user-visible changes;
  - one command runs the tests (`cargo test --workspace`);
  - branch per feature, never commit to `main`, and never push unless the owner invokes `/commitandpush`.
- **Secrets:**
  - API keys come only from environment variables or the macOS Keychain.
  - Keys are never logged, never written to config, and never committed.
  - The repo holds no real session logs (they contain private data). Test fixtures are synthetic.
- **Undocumented inputs:**
  - The Claude Code and Codex on-disk session formats are undocumented and change between releases. loosy-goose learned this the hard way.
  - pelhaim must tolerate unknown records and warn on CLI versions newer than it was tested against.
- **Performance is a non-issue.** The spike ran a full board at the display refresh rate with no optimisation. Don't spend design effort here. (Spike lesson, owner-relayed.)

## 4. Approaches considered (rendering)

The owner chose between three ways to draw the board. **[Owner]**: native window.

| Approach | How | For | Against |
|---|---|---|---|
| **A. Pixel graphics inside the terminal** (iTerm2 inline images / kitty graphics protocol) | Render frames off-screen, then push them as images into the terminal | Stays in the terminal the owner already lives in | Every update re-encodes and re-sends a whole image, so animation is janky. Mouse hit-testing is coarse or absent. Popups and text input fight the terminal. Tied to iTerm2/kitty. Breaks under tmux. |
| **B. Character cells** (a ratatui-style TUI) | Box-drawing, half-block and braille characters | Portable, works over SSH, cheap to build, easy to test | The look is lost: no thin segmented strokes, no 60° diagonals, no custom pixel font, no smooth zoom. Block gaps quantise to whole cells, so the board can't be "very close to the movie aesthetic". |
| **C. Native GPU window launched from the CLI** | `pelhaim` opens a window; a GPU renderer draws the board | Full control of geometry, colour, font and zoom. Real mouse and keyboard. Popups and inset are just more of the scene. Proven by the spike. | You build text input, IME, accessibility and window handling yourself. It doesn't work over SSH. |

**Why C:** the board's value is glanceability, and glanceability here *is* the look: thin segmented tracks, colour per block, crisp tags, a zoom inset. Only C delivers it without compromise. The costs of C (text input, accessibility) are bounded and listed in §9.2. **[Owner]** for the choice; **[Agent's call]** for the reasoning.

## 5. Decided parts (owner)

All of this section is **[Owner]**, restated here so the builder has it in one place.

**Metaphor mapping**

| Board | pelhaim |
|---|---|
| Train | An agent: a subagent, or a session's main agent |
| Line (a bundle of tracks) | A project / GitHub repo |
| Track | A pipeline; trains visibly advance along it |
| Diverging track | A fork or a detach |
| Station / stop | A task, produced by a planning step; dependencies are edges between stations |

**Modes**

1. **Observe.** Tail Claude Code JSONL under `~/.claude/projects` and Codex sessions under `~/.codex/sessions`.
   - View-only. Questions show as alerts, but you answer in the session's own terminal.
2. **Launch.** Run sessions via `claude -p --output-format stream-json` and `codex exec --json`.
   - pelhaim can answer these sessions' questions.
   - There must be a way to take over, or open, a train's live session.
3. **Direct.** pelhaim is the agent runtime.
   - It calls Anthropic's API, or local models via OpenAI-compatible servers.
   - It plans, spawns subagents and runs tools itself.
   - The provider HTTP layer is plumbing. The orchestration is the core.

Modes are configurable. The board is identical in every mode.

**Colour meaning** (owner-approved)

| State | Colour |
|---|---|
| Idle / not yet run | cyan |
| Agent working | green |
| Waiting on the user | yellow |
| Failed / blocked | red |
| Forked / detached | magenta |
| Done | a dimmer grey, e.g. `#85889A` (film-sampled), distinct from idle cyan |

A working train's planned route ahead also lights green, as a cleared route.

**Look**

- **Palette:** muted and matte, derived from the measured film colours, like a traditional train board.
- **Density:** less dense than the spike.
- **Strokes:** smoother (anti-aliased), while keeping the block-segment track language. The owner said the spike "looks good but a bit busy; should be clearer, closer to the more dull / matte colour scheme from the movie… doesn't need to look as blocky; can be a bit smoother while still very very close to the movie aesthetic."
- **Font:** a custom pixel/bitmap font matching the film's lettering, including the hooked "1", and sharp at tag sizes.
- **Alerts:** popups styled on the film's "Maintenance" window and the zoom-frame stack.
- **Chat:** responses use clear checkbox / notification / alert styling.

**Spike lessons** to carry forward

1. Performance is a non-issue.
2. Use one unit system plus a zoom factor.
3. A zoom window is the same scene, clipped.
4. Block colour is derived each frame, never stored.

**Planning**

- The dispatcher plans first.
- The plan is visible before any train departs.
- The owner can edit or approve the plan.

**Stack:** Rust, with a native GPU window launched from the CLI.

### 5.1 Owner rulings after the builder check (2026-11-23)

A builder check on the first draft raised four questions that only the owner could answer. The answers below are **[Owner], 2026-11-23**. Where they conflict with anything elsewhere in this doc, these win; the sections named have been updated to match.

1. **Observed work is assigned to lines by explicit tags, not by cwd.** *Partly reversed 2026-11-24 (§7.7): tags still win, but an untagged session now goes to the line of the git repository its cwd lies in.*
   - The owner's orchestrator runs from `~`, and its subagents record `cwd=~/Developer`, so matching on cwd fails.
   - The orchestrator labels each agent it starts. Untagged work goes to an "Unsorted" line, whose alert offers a one-click "assign to project", remembered per session.
   - The orchestrator session itself appears as a **DISPATCH** line. Its tagged subagent trains branch off it onto their project lines. *Superseded 2026-11-24 by §5.3: DISPATCH is a one-row count strip and the orchestrator draws no train.*
   - The tag format is the agent's call: see §7.7.
2. **Run alongside first, replace later.**
   - In M1, pelhaim only observes the Claude Code orchestrator.
   - From M2 on, pelhaim can itself be the orchestrator. Its planner must follow the owner's routing rules: design and security decisions go to the owner as alerts, every decision is logged, and nothing is pushed or merged without the owner. See §8.5.
   - SQLite stays the source of truth, with a careful two-writer markdown export while both orchestrators run. A single writer becomes possible once pelhaim fully replaces the orchestrator (§7.6).
3. **Git flow: each train gets its own worktree and branch, in Launch mode as well as Direct.**
   - A dependent station branches off its predecessor's branch, so work stacks up.
   - Merges to main go through PRs that the owner merges. pelhaim never merges to main and never commits to main.
   - A merge conflict shows as a red X with an alert.
   - Git writes (branch, worktree, commit on train branches) are their own risk class with their own approval policy. Pushing and opening PRs always need the owner's approval through an alert. See §7.8.
4. **Definition of done.**
   - A step is done when the agent declares it complete **and** the step's automated checks pass (tests and the step's `acceptance` criteria).
   - Steps flagged as decision, merge, prod or security then wait as a yellow alert for the owner's sign-off, in a new "done, awaiting sign-off" state.
   - A long-lived `claude -p` process signals the end of a step with a structured step-complete message, not by exiting. See §7.9.

### 5.2 Owner approval with three changes (2026-11-23)

The owner approved this design on 2026-11-23 with three changes, all **[Owner], 2026-11-23**. They override anything earlier in the doc; the affected sections have been updated.

1. **Auto mode is the default for agents.**
   - Agents run on their own, and only key decisions become stopping points. That is how the owner's sessions work today.
   - By default, file edits and commands inside the train's own worktree are auto-approved.
   - Owner stop points stay:
     - design and security decisions;
     - push and PR;
     - merge;
     - prod;
     - anything outside the worktree;
     - network or secrets;
     - destructive operations.
   - Per-project overrides stay.
   - The policy maps onto the CLIs' own permission modes where they exist. See §10.1.
2. **Auto-caffeinate while running.**
   - Keep the Mac awake while any train is active, and release it when none are.
   - Linux gets an equivalent, or a no-op.
   - A small indicator shows on the board. See §8.6.
3. **Remote control from an iPhone is planned.**
   - A compatible iOS app receives pelhaim alerts as push notifications.
   - The owner can answer them (option buttons, free text) and see a compact board.
   - M1–M4 build only the seam: a remote API on the engine's alert/answer bus.
   - The app, relay and pairing are milestone M5. See §8.7.

### 5.3 Owner decisions after trying M1 live (2026-11-24)

The owner ran the M1 board against real sessions on 2026-11-24 and made three decisions, all **[Owner], 2026-11-24**. They override anything earlier in the doc; §6.5, §6.6, §7.4, §7.7, §7.10, §8.1 and §9.3 have been updated to match.

1. **The orchestrator session gets no train.**
   - DISPATCH becomes a slim header strip, one text row, with a count: `DISPATCH · 6 agents running`.
   - A session is the orchestrator when it is self-tagged `dispatch`, listed in `[dispatch].sessions`, or marked "This is the orchestrator" from its Assign popup.
   - It draws no track, no stations and no forks. Its subagents appear only on their own project lines.
   - Its questions still raise alerts (popup and tray). An untagged subagent of the orchestrator still goes to UNSORTED with an Assign alert.
   - Why: the orchestrator's train cluttered the board, and it exists only to track other sessions.
2. **Stations come from explicit markers only.**
   - Prompts and tool calls never create stations. Before, every prompt became one, which made lines huge.
   - A station is created by a line that is exactly `pelhaim-milestone: <name>` in a session's assistant text or user prompt. The parsing rules are in §7.10.
   - A train with no markers sits on its track with no stations. It still advances one block per tool call, within a bounded track (§7.4), so lines stay controlled.
   - At most `[board].max_stations` stations show per track (default 5). Older ones fold into "+N earlier".
   - Done, failed and waiting still colour the train and its blocks as before.
   - Why: the owner wants lines "more controlled", not massively long at once.
3. **Stations carry arrival times.** "Tasks (stations) should have arrival times based on how long the task will take… realistic on agent hours."
   - A marker may end in an estimate: `pelhaim-milestone: <name> ~<duration>`, e.g. `~20m`, `~1h30m`, `~45s`.
   - pelhaim learns a correction from completed stations: estimate × factor is the ETA. The factor and its rules are in §7.10.
   - The station the train runs to shows a train-board clock, `ARR 14:52` in local time, in the pixel font. An overdue station turns amber (the matte yellow). A station with no estimate shows no clock.

## 6. Architecture

All of §6 is **[Agent's call]** unless marked otherwise.

### 6.1 Shape

```
            ┌───────────────────────────── pelhaim (one process) ─────────────────────────────┐
            │                                                                                  │
 ~/.claude ─┤ observe-claude ─┐                                                                │
 ~/.codex  ─┤ observe-codex  ─┤                                                                │
 claude -p ◄┤ launch-claude  ─┼─► Inbound events ─► ENGINE (reducer, single owner of state) ──► Snapshot (Arc) ─► BOARD (layout + scene, pure) ─► display list ─► GPU renderer ─► window
 codex exec◄┤ launch-codex   ─┤        ▲                │        │                                                   ▲
 provider  ◄┤ direct runtime ─┘        │                │        └─► Store (SQLite event log + decision log writer)     │
            │                          └── Commands ◄───┘ (answer, approve plan, take over, …) ◄── UI input (mouse, keys, IME) ┘
            └──────────────────────────────────────────────────────────────────────────────────┘
```

- **The engine owns all state** and changes it only by folding `Event`s. It is synchronous, does no IO, and is deterministic, so it can be tested headless with fixture event streams.
- **Adapters** (one per mode and tool) turn the outside world into `Event`s and carry out `Command`s. They run as tokio tasks and talk to the engine over channels: an `mpsc` going in, and a `watch` of the latest snapshot coming out.
- **The board layer is pure:** `(Snapshot, ViewState) → DisplayList`. It does layout and colour derivation (colour is derived each frame, per the spike lesson). It knows nothing about the GPU.
- **The GPU renderer** draws a `DisplayList` and nothing else. It is the only crate that touches `wgpu` or `winit`.
- **The UI thread** runs the winit event loop. It owns `ViewState` (pan, zoom, selection, which popup is open, the text being typed), reads the newest engine snapshot each frame, and sends `Command`s.

Why this split:

- The three modes differ only in their adapters, so the board is identical by construction.
- Everything except the last mile can be tested without a GPU.
- The renderer can be swapped, if the crate choice in §9.2 turns out wrong, without touching the engine.

### 6.2 Workspace layout

```
pelhaim/
  Cargo.toml                 # [workspace]
  rust-toolchain.toml
  crates/
    pelhaim-core/            # data model, Event/Command, reducer, plan graph + validation, state derivation. No IO.
    pelhaim-adapters/        # SessionAdapter trait; observe::{claude,codex}; launch::{claude,codex}
    pelhaim-providers/       # Provider trait; anthropic; openai_compat
    pelhaim-runtime/         # direct mode: agent loop, tools, approval gate, sandbox, planner
    pelhaim-store/           # SQLite event log + snapshots; decision-log markdown writer
    pelhaim-board/           # layout, scene building, palette, font glyph data → DisplayList. Pure.
    pelhaim-gpu/             # winit + wgpu renderer of DisplayList, IME, AccessKit bridge
    pelhaim-remote/          # RemoteApi seam (M1); M5: pairing, E2E crypto, relay client   (§8.7)
    pelhaim-relay/           # M5: the small relay + APNs sender (separate binary, deployable)
    pelhaim/                 # the binary: CLI (clap), config, wiring
  assets/font/               # glyph source (text grids) — original work, MIT
  docs/
```

### 6.3 Core traits (sketches, not final signatures)

```rust
/// One per (mode, tool). Observe adapters ignore every Command except Refresh.
#[async_trait]
pub trait SessionAdapter: Send {
    fn kind(&self) -> AdapterKind;                         // ObserveClaude, LaunchCodex, Direct, ...
    fn capabilities(&self) -> Capabilities;                // can_answer, can_take_over, can_spawn, read_only
    async fn run(self: Box<Self>, tx: mpsc::Sender<Event>, rx: mpsc::Receiver<AdapterCommand>) -> Result<()>;
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;                                  // "anthropic", "ollama@localhost:11434", ...
    fn capabilities(&self, model: &str) -> ModelCaps;      // tool calling, parallel tools, context window, vision
    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ProviderEvent>>>;
}

pub trait Tool: Send + Sync {                              // direct mode tools
    fn spec(&self) -> ToolSpec;                            // name, JSON schema, risk class
    fn risk(&self, input: &serde_json::Value) -> Risk;     // ReadOnly | WorktreeWrite | WorktreeExec | GitTrainBranch
                                                           // | OutsideWorktree | Network | Secrets | Destructive  (§10.1)
    async fn call(&self, ctx: &ToolCtx, input: serde_json::Value) -> ToolOutput;
}
```

`Capabilities.read_only = true` for observe adapters. The engine refuses to route an answer `Command` to an adapter that cannot answer, and the UI greys out the answer buttons. This enforces the "observe is read-only" rule in two places.

### 6.4 Async and threading

- A **tokio** multi-thread runtime runs adapters, the provider HTTP layer (`reqwest`, rustls) and the store.
- **The engine** runs on its own tokio task. Its loop is: `recv event → fold → publish snapshot`. It stays single-threaded and owns its state, with no locks inside it.
- **winit** owns the main thread (macOS requires that). It pulls the latest `watch` snapshot each frame and sends `Command`s over a channel.

### 6.5 Persistence

- **Store: SQLite.** Use `rusqlite` with the `bundled` feature, one file at `<data_dir>/pelhaim.db`.
  - Why not Postgres (the owner's default for relational data): this is a local single-user desktop app. SQLite is the right tool and needs no server. Flagged because it departs from the owner's norm.
- **Model: event-sourced, light.**
  - The append-only table `events(id, ts, source, kind, payload_json)` holds pelhaim's *own* events: plans, approvals, answers, launches, alerts and dismissals.
  - Projected tables `projects, tracks, stations, trains, alerts, decisions` exist for fast startup.
  - `station_timings(sample_key, project, estimate_secs, actual_secs, at)` holds the learned arrival-time correction (§7.10, **[Owner], 2026-11-24**). It holds numbers only: the key is `<session key>#<station ordinal>`, never a station name, and `INSERT OR IGNORE` on it makes a catch-up replay record nothing twice. Schema version 2 adds it.
  - On start, load the projections and replay events newer than the last snapshot.
- **Observe mode stores no transcript text.** It stores only:
  - the path,
  - the byte offset it has read to,
  - the derived train and station metadata,
  - alert text: the question itself, so the tray survives a restart.

  On restart pelhaim re-reads the files from disk. It doesn't keep a second copy of private transcripts.
- **Launch and Direct transcripts:**
  - Launch mode: the CLIs persist their own sessions, so pelhaim stores only ids.
  - Direct mode: pelhaim owns the conversation, so it stores messages in `direct_messages`. They stay local and are never logged to stdout.
- **Data dir:** `~/.local/share/pelhaim/`, overridable with `PELHAIM_DATA_DIR`.
  - I prefer this over `~/Library/Application Support` because it is easier for an agent or the owner to find from a shell. **[Agent's call]**, easily reversed.

### 6.6 Config

- **Format and location:** TOML at `~/.config/pelhaim/config.toml`, overridable with `PELHAIM_CONFIG` or `--config`.
  - TOML is native to the Rust ecosystem and readable by hand. The file holds no secrets, ever.
- **Validation:** with `serde` and `deny_unknown_fields`, so a typo is an error rather than silently ignored.

```toml
mode = ["observe", "launch"]          # any of observe | launch | direct; the board is the same

[observe]
claude_home = "~/.claude"             # default honours CLAUDE_CONFIG_DIR like loosy-goose does
codex_home  = "~/.codex"              # default honours CODEX_HOME
active_window = "48h"                 # sessions touched within this window get a train
stall_after = "10m"                   # no writes + not at a turn end → "stalled"

[[project]]
slug = "safeascent"                   # matches `pelhaim-project: safeascent` tags (§7.7)
name = "SafeAscent"                   # the line label
path = "~/Developer/safeascent"       # repo root: worktrees branch from here; an untagged session in this repo lands here (§7.7)
repo = "SebastianFrazier26/safeascent" # optional, for links and PRs
order = 1

[dispatch]
# Observed sessions that are orchestrators: drawn on the DISPATCH line (§7.7).
# Matched by session id, or by a `pelhaim-project: dispatch` tag in the session's first user message.
sessions = []

[launch]
claude_bin = "claude"
codex_bin  = "codex"
max_concurrent = 4                    # global; per-project override allowed
takeover_terminal = ["open", "-a", "iTerm", "--args"]   # argv template; see §8.2.4

[direct]
default_provider = "anthropic"
[direct.providers.anthropic]
kind = "anthropic"
model = "<set by owner>"              # no default model id is hard-coded in the binary
key = { env = "ANTHROPIC_API_KEY" }   # or { keychain = "pelhaim/anthropic" }
[direct.providers.local]
kind = "openai_compat"
base_url = "http://127.0.0.1:11434/v1"
model = "<set by owner>"

[approval]                            # AUTO MODE is the default (owner, 2026-11-23; §10.1)
                                      # per risk class: allow | ask | deny
read_only = "allow"
worktree_write = "allow"              # edits inside the train's own worktree
worktree_exec = "allow"               # commands run with cwd = worktree, not touching anything outside it
git_train_branch = "allow"            # branch, worktree, commit on the train's own branch
outside_worktree = "ask"              # any read-write path outside the worktree
network = "ask"
secrets = "ask"                       # .env, keychains, credential files, key-shaped env vars
destructive = "ask"                   # §10.1 list: reset --hard, clean, branch -D, recursive rm, history rewrite, …
# Fixed, not configurable: push and open-PR always need an owner alert; merge to main never happens;
# design/security/merge/prod steps always stop for sign-off (§7.9).

[project_overrides.safeascent.approval]   # per-project overrides (owner, 2026-11-23); same keys as [approval]
network = "deny"

[keep_awake]                          # §8.6
enabled = true
include_observed = true               # observed working sessions also hold the Mac awake
keep_display_awake = false            # adds -d

[remote]                              # §8.7. The seam ships from M1; the bridge is off until M5
enabled = false

[git]
branch_prefix = "pelhaim/"            # train branches: pelhaim/<project>/<station-id>-<train#>
worktree_root = "~/.local/share/pelhaim/worktrees"

[decision_log]
path = "~/Developer/DECISION-LOG.md"
write = "on_click"                    # on_click | off   (never "auto" in v1)

[board]
palette = "matte"                     # matte | film | high_contrast
motion = "full"                       # full | reduced
float_alert_lines = true              # lines with open alerts float up
max_stations = 5                      # stations shown per track, 1–20; older fold into "+N earlier" (§7.10)
auto_projects = true                  # untagged sessions go to their cwd's repo line; unknown repos and tags get lines (§7.7)
```

### 6.7 CLI surface

The CLI uses `clap`.

| Command | What it does |
|---|---|
| `pelhaim` | Open the board with the configured modes. |
| `pelhaim observe` | Open the board in observe mode only, even if config says more. Safe default for first run. |
| `pelhaim plan --project X "goal"` | Open the board, with the dispatcher planning that goal (Launch or Direct). |
| `pelhaim launch --project X [--tool claude\|codex] "prompt"` | One train, no planning. |
| `pelhaim doctor` | Prints CLI versions found vs tested, config validity, key presence (yes/no, never the value), data-dir health and GPU adapter. |
| `pelhaim decisions [--open]` | Prints the structured decision log. |
| `pelhaim --headless --dump-scene out.json` | Builds the scene from current state without a window. Used by tests and agents. |

## 7. Data model

All of §7 is **[Agent's call]**. The concept mapping comes from §5 and is **[Owner]**.

### 7.1 Entities

```rust
struct Project {             // = Line (a bundle of tracks)
    id: ProjectId, slug: String, name: String, root: PathBuf, repo: Option<String>, order: i32,
    kind: LineKind,          // Project | Dispatch | Unsorted   (§7.7)
    tracks: Vec<TrackId>,
}

struct Track {               // = Pipeline
    id: TrackId, project: ProjectId,
    kind: TrackKind,         // Session{tool, session_id} | PlanChain{plan, chain_idx} | Branch{parent: TrackId, at: StationId}
    stations: Vec<StationId>,// left→right order
    origin: Option<ForkId>,  // set if this track diverged from another
}

struct Station {             // = Task
    id: StationId, track: TrackId, title: String, detail: String,
    depends_on: Vec<StationId>,   // may cross tracks (drawn as junction tracks)
    status: StationStatus,        // Pending | Ready | Active | Checking | AwaitingSignoff(AlertId) | Done | Failed | Skipped
    train: Option<TrainId>,       // the train berthed or assigned here
    source: StationSource,        // Plan(step_id) | Inferred(observe heuristic)
    signoff: Vec<SignoffFlag>,    // from the plan step: Decision | Merge | Prod | Security   (§7.9)
    branch: Option<String>,       // train branch for this station, Launch/Direct only   (§7.8)
}

struct Train {               // = Agent
    id: TrainId,
    number: u16,             // display id; see 7.3
    track: TrackId,
    position: Position,      // { leg: StationId → StationId, block: u16 }  — progress along the leg
    state: TrainState,       // Idle | Working | Checking | Waiting(AlertId) | DoneAwaitingSignoff(AlertId)
                             // | Failed(reason) | Detached | Done | Stalled
    tag: TagResolution,      // Tagged(slug) | Untagged | MisTagged(raw) | Assigned(slug, by_owner)   (§7.7)
    worktree: Option<Worktree>, // { path, branch, base_branch } — Launch/Direct only   (§7.8)
    runtime: RuntimeRef,     // ObservedClaude{path, session_id, agent_id?} | ObservedCodex{path} | LaunchedClaude{pid, session_id}
                             // | LaunchedCodex{pid, thread_id} | Direct{conv_id, provider, model}
    parent: Option<TrainId>, // spawner
    label: String,           // subagent description / session title
    counters: Counters,      // tool calls, tokens, cost if reported, started/last_seen timestamps
}

struct Fork {                // = Diverging track
    id: ForkId, parent_track: TrackId, at_block: u16, child_track: TrackId,
    kind: ForkKind,          // Subagent | SessionFork | Detached(takeover) | Background
    rejoin: Option<StationId>,   // where the child's result merges back (drawn as an "X" crossover)
}

struct Alert {
    id: AlertId, train: TrainId, project: ProjectId,
    kind: AlertKind,         // Question{questions, multi} | Permission{tool, input_summary} | Result{summary}
                             // | Failure{reason} | PlanReview{plan} | Stalled
                             // | Signoff{station, flags, checks} | GitApproval{Push|OpenPr, branch}
                             // | MergeConflict{branch, base, files} | AssignProject{session}
    created: Timestamp, state: AlertState,   // Open | Tray (dismissed) | Answered{answer, by} | Expired
    answerable: bool,        // false for observed sessions
    decision: Option<DecisionId>,
}

struct Plan {
    id: PlanId, project: ProjectId, goal: String,
    steps: Vec<PlanStep>,    // { id, title, detail, depends_on, agent_hint: {tool|provider, model?},
                             //   acceptance: Vec<Check>, signoff: Vec<SignoffFlag> }
                             // Check = Command{argv, cwd: worktree} | Manual{text}   (§7.9)
    status: PlanStatus,      // Draft | Approved | Running | Done | Abandoned
    revision: u32,
}

struct Decision {            // mirrors one DECISION-LOG.md entry
    id: DecisionId, project: ProjectId, date: NaiveDate,
    title: String, why: String, who: Who,   // Owner | AgentDefaultOwnerConfirmed | AgentDefaultUnconfirmed
    still_to_do: Option<String>, reversal_of: Option<DecisionId>,
    source_alert: Option<AlertId>, written_to_md: bool,
}
```

### 7.2 State derivation

Colour is never stored (spike lesson). Each frame, block colour is derived from `TrainState`, `StationStatus` and `Position`:

- **Blocks behind** a train, back to its origin, are `Done` grey.
- **The train's occupied blocks** take the train state colour. A train is 3 blocks long; while working, 5.
- **Blocks ahead,** up to the next station the train is cleared to, are the cleared-route green.
  - Cleared-route green is the same hue, drawn at reduced lightness, so the train itself stays distinct. **[Agent's call]**: the owner asked for green and did not specify the shade.
- **Everything else** is idle cyan.
- **A detached fork's whole child track** is magenta.
- **New states from the 2026-11-23 rulings:**
  - `Checking` (acceptance checks running) is working green. The tag shows a small check mark in the inset and the a11y name.
  - `DoneAwaitingSignoff` is yellow: it is waiting on the owner. The train sits berthed at the station with its platform bar yellow, and is not grey until signed off. **[Owner]** for the state (2026-11-23); the colour follows the owner's colour table.

### 7.3 Train numbers

**[Agent's call]**

- A train's number is its **departure time as HHMM** in local time, e.g. `1423`. This borrows the real NYCT convention noted in the visual spec. It also tells the owner at a glance when an agent started.
- A collision gets the next free minute. The full id is in the inset and the popup.
- Times before 10:00 render as 3 digits, which matches the film's 3- and 4-digit tags.

### 7.4 Train movement

**[Agent's call]**

Real agents don't report percentage progress, so pelhaim uses a rule that is honest about that:

- Each tool call, or each completed model turn, steps the train forward **one block** along its leg.
- A train never passes the next station until that station's task is done. It stops one block short and waits.
- When the task completes, the train berths at the station (the platform bar lights), then departs on the next leg.

So a train's motion means activity, never invented progress.

**Observe mode, 2026-11-24 (§5.3):** stations come only from milestone markers (§7.10).

- A track is one leg of `LEG_BLOCKS` (10) blocks per kept station, plus the open leg the train runs on. With no marker, it is just the open leg, so a track is never longer than `max_stations` legs.
- A tool call steps the train one block, up to one block short of the leg's end. A train with no station ahead waits there: the track is bounded, and the tool-call counter in the inset keeps counting.
- A marker closes the current station and opens the next one at the end of a new leg; the train starts that leg at block 0.
- A main session's turn end stops the train where it is (it does not jump to a station). A subagent's finish berths it at the end of its leg, which closes its station.

### 7.5 Plan graph

- A plan is a **DAG**. On every edit, validate:
  - ids are unique;
  - every dependency exists;
  - there are no cycles (Kahn's algorithm);
  - there is at least one root.
- **Layout onto tracks:**
  1. Topological layering gives each step a column, i.e. an x-slot.
  2. A greedy path cover splits the DAG into chains: take the longest remaining path, repeat. Each chain is one track in the project's line.
  3. A dependency between chains is drawn as a solid diagonal junction track, using the film's sharp-cornered ~60° diverging-track geometry.
- **Readiness:** a step is Ready when all its dependencies are Done. "Done" means fully done by §7.9: checks passed and, if flagged, signed off. A station in `AwaitingSignoff` does not release its dependants.
- **Dispatch:** the dispatcher starts one train per Ready station, up to `max_concurrent`.

### 7.6 Decision log

**[Agent's call]**. This is the part the owner should check most carefully.

- **Source of truth:** the `decisions` table in SQLite. `DECISION-LOG.md` is an *export target*, written only on an explicit click (`write = "on_click"`).
- **Format:** entries follow the file's own rules, read today:
  - newest first, at the top of that project's `### Decided` list;
  - a bold one-line title with the date;
  - "Why" and "Who" lines, with Who one of the three listed forms;
  - `[REVERSAL]` when one decision undoes another;
  - a new project gets a new `## <Project>` section, inserted before `## Standing rules`.
- **Writing safely:**
  1. Re-read the file immediately before writing.
  2. Insert by locating the heading. Never rewrite other content.
  3. Write to a temp file in the same directory, then `rename` it into place.
  4. Keep the previous version as `DECISION-LOG.md.pelhaim-bak`.
  5. If the hash changed between read and rename, retry once, then raise an alert instead of writing.
- **Why so careful:** the file today is also maintained by the owner's Claude Code orchestrator, so two writers exist. See Risks R6.
- **[Owner], 2026-11-23:** keep SQLite as the source of truth, with this careful two-writer export, for as long as pelhaim runs alongside the Claude Code orchestrator.
  - **Later:** once pelhaim fully replaces the orchestrator, a single writer becomes possible. pelhaim would then be the only writer and could export automatically after each logged decision. Switching is an owner decision, made when it happens. Until then `write = "auto"` does not exist.
- **Open decisions:** pelhaim reads each project's `### Open decisions (need you)` list too, and shows the count as a small yellow badge on the line label. This is read-only.

### 7.7 Project tags (assigning work to lines)

**[Owner], 2026-11-23:** observed work is assigned to lines by explicit tags, never by cwd. The tag format is the **[Agent's call]**.

**[Owner], 2026-11-24 — reversal, in part:** "the trains are currently asking which project they're for instead of auto-registering them based on the worktree." Sessions now auto-register to a project from where they run. Explicit tags stay and take priority; the cwd's repository is the fallback before Unsorted. This reverses the 2026-11-23 rule above for untagged sessions only. The mechanics below the owner's rule are the **[Agent's call]**.

**Format**

- The first line of an agent's spawn prompt is exactly:
  ```
  pelhaim-project: <slug>
  ```
  - `<slug>` is lowercase `[a-z0-9-]+`, matched against `[[project]].slug` in config.
  - Leading whitespace and a trailing `\r` are ignored; nothing else is. The key is case-sensitive.
  - The reserved slug `dispatch` marks an orchestrator session.
- **Where pelhaim reads it:**

  | Source | Where the tag lives |
  |---|---|
  | Claude Code subagent | The parent's `Agent` tool_use: `input.prompt` (the input also carries `description`, used as the train label). Linked to the subagent file by `toolUseId` in `agent-<id>.meta.json`. Fallback: the first `user` record in the subagent's own JSONL, which is that prompt. |
  | Codex | The first line of the prompt passed to `codex exec`, read from the first user `response_item:message` in the rollout. |
  | Any session tagging itself | The first line of the session's first real user message (not meta, not a tool result). |

- **Resolution order (2026-11-24):**
  1. An explicit owner assignment from the UI. A project assignment also covers the session's later untagged subagents, as before.
  2. The subagent's spawn-prompt tag.
  3. The session's self-tag. A session listed in `[dispatch].sessions` goes to DISPATCH whatever its tag.
  4. **The session's repository.** The recorded cwd (Claude JSONL `cwd`; Codex `session_meta.payload.cwd`) is canonicalised (symlinks resolved) and walked up to the nearest `.git`:
     - a `.git` directory: that directory is the repository;
     - a `.git` file (a linked worktree, `gitdir: …/.git/worktrees/<name>`): `<gitdir>/commondir` leads to the main repository's git dir, whose parent is the repository. A bare `<name>.git` git dir is its own root, named without `.git`. A `.git` file with no `commondir` (a submodule) or that is not a pointer names its own checkout.
     - The project is the main repository's directory name, slugged (ASCII lowercased, each run of other characters one `-`, trimmed): `~/Developer/SafeAscent-r7v2`, a worktree of `~/Developer/SafeAscent`, gives `safeascent`.
     - It matches a `[[project]]` whose `path` (tilde-expanded, canonical) is the repository root, else one whose `slug` is the repository slug. If none matches, a line is **auto-created** for it, named after the repository directory, so the board works with no config.
     - Only read-only file reads (`fs::canonicalize`, `fs::metadata`, `File::open` of `.git` and `commondir`, at most 4 KiB each). No git process runs and nothing is written; the read-only proof tests cover it. The observer caches the result per cwd for the run.
  5. Unsorted, which now means no usable tag **and** no repository: for example the orchestrator's subagents working from `~/Developer` itself.
  - A subagent with no tag of its own inherits no tag: it is placed by its own cwd's repository, else Unsorted. It never inherits DISPATCH. **[Agent's call]**
  - An `AssignProject` alert is raised only for a session that ends up on Unsorted. A session that moves off Unsorted (for example when its cwd arrives after discovery) has its open Assign alert expired.
- **Auto-created lines (2026-11-24, [Agent's call]):** with `auto_projects` on, a well-formed tag whose slug is not in config also gets its own line, named by the slug; a repository with the same slug later renames it to the repository's spelling. Configured lines always show, in `order`; auto lines follow, by slug, and show only while a track is on them. They are offered as Assign options. A decision logged on an auto line has no `log_section`, so its export fails visibly with a Failure alert until the project is added to config.
- **`[board].auto_projects`** (default `true`) turns all of step 4 and auto-created lines off, restoring the 2026-11-23 behaviour: tags only.
- **Mis-tagged:** with `auto_projects = false` only, a well-formed tag whose slug is not in config. The train goes to Unsorted with a red-outlined tag label showing the unknown slug, and the assign alert pre-fills the closest slug by edit distance.

**Lines**

- **DISPATCH line:** always the top band. It holds orchestrator sessions: those listed in `[dispatch].sessions`, or self-tagged `pelhaim-project: dispatch`, plus a one-click "this is the orchestrator" action on any Unsorted session.
  - **[Owner], 2026-11-24 (§5.3):** DISPATCH is a one-row header strip, `DISPATCH · N agents running`, and the orchestrator draws no train, track, stations or forks. N counts the active (not idle, done, stalled or failed) trains with an orchestrator among their ancestors, so a subagent's own subagents count too. The count turns yellow while an orchestrator has an open alert.
  - Each tagged subagent spawned from a DISPATCH session runs on its project's line as its own track, starting at the line's left edge; no fork is drawn back to DISPATCH. The orchestrator's questions still raise popups and tray entries.
  - *Before 2026-11-24:* each such subagent left the DISPATCH track on a diverging track that crossed down onto its project's line.
- **Unsorted line:** the bottom band. Each session on it (no tag and no repository) raises an `AssignProject` alert listing the projects as one-click options.
  - The answer is stored in SQLite per session id (and per subagent id), and survives restarts. It moves the train and every later subagent of that session.
- **The orchestrator side:** the owner's orchestrator must be told to prefix each spawn prompt with the tag. That is a one-line addition to the owner's global `CLAUDE.md` delegation rules, in the setup repo. pelhaim's own Launch and Direct modes always write the tag themselves.

### 7.8 Git flow

**[Owner], 2026-11-23**, for the rules. Mechanics are the **[Agent's call]**.

**Rules (owner)**

- Every Launch-mode and Direct-mode train works in **its own git worktree on its own branch**.
- **Stacking:** a dependent station branches off its predecessor's branch, so work stacks up. A root station branches off `main`.
- **Main is the owner's:** merges to main go through PRs that the owner merges. pelhaim never merges to main and never commits to main.
- **Conflicts:** a merge conflict shows as a red X crossover on the board, plus an alert.
- **Approval:** git writes (create branch, add worktree, commit on a train branch) are their own risk class, `git_train_branch`, with its own approval policy in config. Under auto mode (§10.1) it defaults to allow. **Pushing and opening PRs always need the owner's approval through an alert.** This is not configurable.

**Mechanics (agent's call)**

- **Branch name:** `pelhaim/<project-slug>/<station-id>-<train#>`. **Worktree path:** `<worktree_root>/<project-slug>/<branch-leaf>`.
- **Lifecycle:**
  1. When a station becomes Ready, pelhaim runs `git worktree add -b <branch> <path> <base>` in the project root, under the `git_train_branch` policy. The base is `main` for a root station, or its predecessor station's branch.
  2. The agent works with that worktree as its cwd. Launch mode: `claude -p` runs with the worktree as its cwd; Codex gets `-C <worktree>`. Direct mode: tool confinement root = the worktree.
  3. Commits happen on the train branch only. pelhaim refuses any command, its own or an agent tool call it mediates, whose effect is a commit to `main`, a merge into `main`, or a push without approval. In Launch mode pelhaim cannot mediate the CLI's own tool calls, so it relies on the CLI's permission settings and the owner's global rules, and it checks after each step that `main` has not moved. If it has, it raises a red alert.
  4. When a station is fully done (§7.9), its branch is kept. The worktree may be removed (owner setting) once no dependant still needs it as a base.
- **Multiple predecessors:** a station that depends on two branches is based on the first predecessor's branch, and pelhaim merges the second into it (a merge commit on the train branch, never on main) before the agent starts.
  - A conflict here stops the station: the board draws a red X between the two tracks, and a `MergeConflict` alert lists the files, with actions `[Open worktree in terminal]` and `[Retry merge]`.
- **Upstream changed:** if a predecessor branch gets new commits after a dependant has branched (for example a fix after sign-off), the dependant shows a small "base moved" mark. Rebasing is offered as an action, never done automatically.
- **Push and PR:**
  - When a stack's last station is done, the plan's project raises a `GitApproval{OpenPr}` alert showing branch, base, commit list and diffstat.
  - On approval pelhaim runs `git push -u origin <branch>` and `gh pr create` with the base set to the predecessor's branch (a stacked PR) or `main`.
  - pelhaim never runs `gh pr merge`.
- **Observe mode** does none of this: it never touches git.

### 7.9 Definition of done

**[Owner], 2026-11-23:** a step is done when the agent declares it complete **and** the step's automated checks pass (tests and the step's `acceptance` criteria). Steps flagged as decision, merge, prod or security then wait for the owner's sign-off as a yellow alert, in a "done, awaiting sign-off" state.

**State sequence for a station**

```
Active ──agent declares complete──► Checking ──all checks pass──► (no flags)  Done
                                        │                  └──► (flagged)  AwaitingSignoff ──owner signs off──► Done
                                        │                                         └──owner rejects──► Active (with the owner's note sent to the train)
                                        └──a check fails──► Active (failure output sent to the train, attempt n+1)
                                                             └── after max_check_attempts (default 3) ──► Failed (red)
```

- **Checks:** each `acceptance` entry is a `Check`.
  - `Command{argv}` runs in the train's worktree under the `worktree_exec` policy, with a timeout. Exit 0 = pass. The plan's project can set a default check, e.g. `cargo test --workspace`.
  - `Manual{text}` cannot be automated, so it turns the station into a sign-off step regardless of flags.
  - Checks are run by pelhaim, not by the agent, so "tests pass" is never just the agent's word. **[Agent's call]**
- **Sign-off flags:** `decision`, `merge`, `prod`, `security`, set per step by the planner or the owner while editing the plan. The `Signoff` alert shows the agent's completion summary, the check results, the diffstat, and buttons `[Sign off]`, `[Reject with note]` and `[Log decision]`.
- **Observe mode** has no checks: an observed train's turn end is just a berth (§8.1).
- **Auto mode** (**[Owner], 2026-11-23**; §10.1) governs the tool calls inside a station and does not change this definition of done. Edits and commands inside the worktree raise no alerts. The stop points are these sign-offs, plus the §10.1 stop classes and the §8.5 routing rules.

**How a step ends in a long-lived process** (agent's call)

A process exit can't mean "step done", because a `claude -p --input-format stream-json` process stays alive between steps and a Codex process may exit mid-step to wait for an answer. Instead the agent declares completion with a structured signal:

- **Direct mode:** a built-in tool `step_complete{ station_id, summary, artifacts?: [paths], notes_for_owner? }`. Calling it ends the train's loop for that station and starts Checking.
- **Launch mode, Claude:** the preferred route is the same `step_complete` tool, exposed to the session as an MCP tool served by pelhaim over a local stdio MCP server (passed with `--mcp-config`, verified 2026-11-23 to exist). The tool call appears in the stream-json output with its input, which is the signal.
  - Fallback, if MCP proves awkward in the M2 spike: the session's prompt instructs the agent to end the step with a final message whose last line is a single JSON object `{"pelhaim":"step_complete","station":"<id>","summary":"…"}`. pelhaim parses that line and ignores look-alikes inside code fences.
- **Launch mode, Codex:** the same final-line JSON convention, read from the turn's last agent message. `--output-schema` (verified) can make the whole final message that object instead. The M2 spike picks one.
- **Silence is not completion:** a turn that ends with neither signal leaves the station Active and raises a low-priority `Result` alert: "Train 1148 stopped without declaring the step complete". The owner can then nudge it or mark the step complete by hand.

### 7.10 Milestone markers, stations and arrival times

**[Owner], 2026-11-24** (§5.3) for the rule that stations come only from markers and carry arrival times. The parsing details, the factor's constants and the display are the **[Agent's call]**.

**Marker format**

```
pelhaim-milestone: <name>
pelhaim-milestone: <name> ~<duration>
```

- **Own line:** the marker must be a whole line. Leading spaces and tabs and one trailing `\r` are ignored, as for the tag; nothing else is. The key is case-sensitive and takes exactly one space after the colon. Text before the key on the same line (`` `pelhaim-milestone: x` ``, `> pelhaim-milestone: x`, prose) does not fire.
- **Any line:** unlike the tag, a marker counts on any line of the text, so an agent can report several in one message, in order.
- **Not in code fences:** lines inside a fenced block (` ``` ` or `~~~`, three or more; closed by the same character, at least as many, and nothing but spaces after) are ignored. An unclosed fence hides the rest of that text block.
- **Where it is read:** Claude Code assistant `text` blocks and real user prompts (main and subagent, not meta, tool results or harness notices); Codex assistant `output_text` messages and owner prompts. Never tool inputs, tool results, harness-injected messages, or Codex's `task_complete` echo of the last message.
- **Name:** sanitised like a title (controls and ANSI sequences become spaces, whitespace collapses, ends trimmed), then cut to **32** chars with `…`. An empty name is no marker, even with an estimate.
- **Estimate:** an optional last word `~<duration>`, after a space. The duration is one or more `<digits><unit>` groups with units `h`, `m`, `s` in that order, each at most once, e.g. `~20m`, `~1h30m`, `~45s`, `~2h5m10s`. It must be above zero and at most 7 days. Anything else (`~soon`, `~0m`, `~30m1h`, `~1d`) stays part of the name. Config's `parse_duration` takes one unit only, so the marker has its own parser.
- **Duplicates:** a name already seen on the same track, folded stations included, is a no-op: no station, no timing, no change to the estimate. A session reset clears the seen names.

**Stations**

- A marker closes the track's newest station (Done) and opens a new Active one, titled with the name, which the train then runs to (§7.4).
- At most `[board].max_stations` stations (default 5, 1–20) are kept per track. Older ones are dropped from the front and counted in `Track.folded`, drawn as `+N earlier` at the start of the track and as the first row of the inset's station list. The train's leg and any fork point on the track shift with the drop.
- A subagent starts with no station; its spawn description is its tag label only.
- The platform bar is the train's state colour while the train stands at it, amber while the station is overdue, done grey once passed, and idle otherwise.

**Arrival times**

- A station's task runs from its marker to the next marker on the track, or to the train finishing (a subagent's finish). A main session going idle does not end it.
- `ETA = marker time + estimate × factor`, fixed when the marker is read. A station without an estimate has no ETA and shows no clock.
- **Display:** only the station the train runs to (the Active one) shows `ARR HH:MM`, in local time, in the 5x7 pixel font after its title; the inset shows the same after the station's name. It is the glyph colour, and the matte yellow once the clock is past the ETA ("overdue"). Done stations show no clock.
- **Learning:** when a station with an estimate is closed, `(estimate, actual)` is recorded once, with the track's project slug (none on DISPATCH or UNSORTED), as an owner-side projection (§6.5). Numbers only; never the name.
- **Factor:** the median of `actual / estimate` over the newest 20 samples of the station's project, once it has at least 5; else over the newest 20 samples overall, once there are at least 5; else 1. It is clamped to 0.25–8 so a station left open overnight cannot wreck every later ETA. The engine keeps the newest 1000 samples in memory.
- A restart replays transcripts, so a station's ETA after a restart may use samples recorded after its marker. Accepted: the clock is a guide, not a record.

## 8. Event flow per mode

### 8.1 Observe mode

The adapter is `observe::claude` or `observe::codex`. Its capabilities are `read_only = true` and `can_answer = false`. **[Owner]** for read-only; the rest is **[Agent's call]**.

**Discovery and tailing**

- Watch the homes with the `notify` crate (FSEvents on macOS), with a 2 s poll as a fallback.
- Keep a byte offset per file. Read only up to the **last newline**: a final line without its newline is a record still being written. This is a loosy-goose lesson (`model.py::snapshot`).
- Parse each line as JSON. Skip invalid or non-object lines with a warning, never a crash.

**Claude Code** (`<claude_home>/projects/<cwd-slug>/<sessionId>.jsonl`)

- **Project:** by explicit tag, then by the cwd's git repository, per §7.7. **[Owner], 2026-11-23; reversed in part 2026-11-24.**
  - The first draft matched the recorded `cwd` against project paths. That was dropped on 2026-11-23, and restored on 2026-11-24 as the fallback after tags: the cwd is resolved to its main repository (§7.7 step 4). A cwd of `~/Developer`, which is not a repository, still says nothing about the project.
  - `cwd` is also shown in the inset.
- **History order:** follow `parentUuid` chains, not line order. Parallel tool calls branch the chain (`claude.py::_chain` and `_splice`).
  - For board purposes, a simpler rule is enough: count records by type and track open tool calls by id.
  - The full chain walk matters only if pelhaim ever reconstructs text. It shouldn't need to.
- **Subagents** (verified on disk 2026-11-23, Claude Code 2.1.331):
  - Transcripts live at `<project-slug>/<sessionId>/subagents/agent-<agentId>.jsonl`.
  - Next to each is `agent-<agentId>.meta.json`, with fields including `agentType`, `description`, `toolUseId`, `model` and `finished`.
  - `toolUseId` links the subagent to the parent's `Agent` tool_use. That gives the fork point (parent train, block) and the train label (`description`).
  - Records inside carry `isSidechain: true` and `agentId`.
- **Signals to events:**

  | Record | Event |
  |---|---|
  | `assistant` with a `tool_use` | `TrainStep` (one block). Remember the call id as open. |
  | `user` with `tool_result` for that id | Close the call. |
  | `tool_use` named `AskUserQuestion` (verified: `input.questions[]` with `question`, `header`, `multiSelect`, `options[{label, description}]`) | `AlertRaised(Question, answerable=false)`. Closes when the matching `tool_result` arrives; then `AlertAnswered(by="terminal")` with the result text as the answer. |
  | `tool_use` named `Agent` | Expect a subagent; the fork appears when its `subagents/` file appears. |
  | Assistant text with no tool calls, followed by silence | Turn ended: the train berths and goes Idle at its station. Optionally a `Result` alert; config, default off for observed. |
  | A `tool_use` open longer than ~20 s with no further writes | `Waiting(probable permission prompt)`, shown yellow with the label "probably waiting on you in the terminal". |
  | No writes for `stall_after`, and not at a turn end | `Stalled` ("Trying to contact train 1423…", a nod to the film's maintenance pane). |
  | `system` records whose subtype names an API error (exact subtypes [Open]) | `Failed`. |

  Permission prompts are probably not written to the JSONL (unverified), which is why the open-tool-call heuristic exists.
- **Stations in observe mode** (no plan exists): **[Owner], 2026-11-24** (§5.3), only `pelhaim-milestone:` markers create stations (§7.10). Prompts and tool calls never do, and the orchestrator on DISPATCH draws none.
  - *Before 2026-11-24:* each owner prompt started a station titled with its first ~40 characters, and each subagent's single station was its `description`. Dropped because it made lines huge.
- **Codex project:** also by tag (§7.7), read from the first user message in the rollout.

**Codex** (`<codex_home>/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl`)

- Each line is `{timestamp, type, payload}`. The first line is `session_meta`, whose payload includes `cwd`.
- Record types seen today across the 20 newest files include:
  - `response_item` (payload types `message`, `reasoning`, `function_call`, `function_call_output`, `custom_tool_call`, `custom_tool_call_output`);
  - `event_msg` (`task_started`, `task_complete`, `turn_aborted`, `token_count`, `agent_message`);
  - `turn_context`, `world_state`, `token_usage_record`, `inter_agent_communication_metadata`.
- A `compacted` record replaces earlier history (loosy-goose `codex.py`).
- **Mapping:**

  | Record | Event |
  |---|---|
  | `function_call` / `custom_tool_call` | `TrainStep` |
  | `event_msg:task_started` | `Working` |
  | `event_msg:task_complete` | Berth, then `Idle` |
  | `event_msg:turn_aborted` | `Failed("aborted")` |

- **Codex subagents:** `inter_agent_communication_metadata` suggests a way to recognise them, but I have not verified it. **[Open]** Q4.
- **Codex questions and approvals:** whether these appear in the rollout file is unverified. **[Open]** Q5.

**Version guard:** each adapter has a `TESTED_VERSIONS` list, like loosy-goose.

- When a session's recorded version (Claude records carry `version`; Codex `session_meta` carries the CLI version) is newer, the line label shows a small "format untested" tick.
- `pelhaim doctor` explains the tick. Parsing continues best-effort.

**Answering:** alerts from observed sessions show the options, but the buttons are replaced by "Answer in terminal" and a **copy-session-id / reveal-cwd** action.

- pelhaim never focuses, types into or signals the other process.
- When the answer appears in the log, the alert flips to Answered, and the popup offers **[Log decision]**.

### 8.2 Launch mode

The adapter is `launch::claude` or `launch::codex`. Its capabilities are `can_answer = true` and `can_take_over = true`.

All flags in this section were verified 2026-11-23 against `--help` unless marked unverified. The builder must re-run `--help` at build time. `pelhaim doctor` should compare.

#### 8.2.1 Claude

- **Spawn:**
  ```
  # cwd = the train's worktree (§7.8)
  claude -p --output-format stream-json --input-format stream-json --verbose \
         --session-id <uuid> -n "pelhaim <train#> <station>" \
         --mcp-config <pelhaim step_complete server> \
         --permission-mode auto --disallowedTools "Bash(git push *)" "Bash(gh pr merge *)" \
         [--model …]          # auto mode default, owner 2026-11-23; mapping in §10.1
  ```
  - The first prompt line is always `pelhaim-project: <slug>` (§7.7). The prompt ends with the step-complete instructions (§7.9).
  - `--session-id <uuid>` (verified) lets pelhaim pick the id up front, so it knows the JSONL path before the first byte arrives.
  - `--input-format stream-json` (verified: "realtime streaming input") keeps stdin open, so pelhaim can send follow-up user messages, i.e. answers, without respawning.
  - `--verbose` is listed (verified). Whether stream-json in `-p` still requires it is unverified; check it.
  - Also worth using:
    - `--include-partial-messages`, for live text in the inset;
    - `--forward-subagent-text` (verified; it sets `parent_tool_use_id` on forwarded subagent messages), so subagents appear as trains straight from the stream;
    - `--json-schema` (verified), for the planner's structured plan output;
    - `--max-budget-usd` (verified), as a cost guard.
- **Questions and permissions:**
  - `--permission-prompts host` is the default. Per help (verified), it sends permission prompts to "the SDK host or `--permission-prompt-tool`".
  - The `--permission-prompt-tool` flag is referenced there but **not listed as its own entry in `--help`** (verified absence), so treat its exact form as unverified.
  - The design wants pelhaim to be "the SDK host": it receives permission requests on stdout and replies on stdin. The exact control-message shapes, and whether `AskUserQuestion` is available and answerable in `-p` mode, are **unverified**.
  - **First task of M2 is a spike** that runs a real session and records the stream. **[Open]** Q6.
  - Fallback if no host protocol is usable: `--permission-mode` plus `--allowedTools` set from the §10.1 policy, and questions answered by sending the answer as the next user message on stdin.
- **Events:** each stream-json line is parsed into the same internal `Event`s as observe mode: tool use → step, result → berth, and so on.
  - The adapter *also* tails the session's own JSONL as a cross-check, which makes takeover seamless (§8.2.4).

#### 8.2.2 Codex

- **Spawn:**
  ```
  codex exec --json -C <train worktree> -s workspace-write --approve-for-me [-m <model>] [--output-schema …] "pelhaim-project: <slug>\n…"
  ```
  All of these flags were verified. `-C` points at the train's worktree, not the project root (§7.8).
- **Local models through Codex:** `--oss` and `--local-provider lmstudio|ollama` are verified. Launch mode can therefore also drive local models through Codex, independently of Direct mode.
- **Process life:** `codex exec` is one-shot; the process exits after the task.
  - A follow-up or answer is a new process: `codex exec resume <SESSION_ID> "<answer>"` (verified: `resume [SESSION_ID] [PROMPT]`). The train stays the same; only the process changes.
- **Approvals:** in exec mode, approval requests can't be answered interactively through `--json` stdout (inferred: the stream is one-way).
  - Options are a sandbox and approval policy set at launch from the owner's policy, or `--approve-for-me` (verified; it routes approvals to automatic review in workspace-write).
  - `codex app-server` (verified to exist, marked "[experimental]") may offer a two-way protocol with approval requests. Evaluating it is **[Open]** Q7.

#### 8.2.3 Flow

```
owner approves plan ─► engine: station Ready ─► git: worktree + branch off base (git_train_branch policy, §7.8)
   ─► Command::Launch{station, tool, worktree} ─► adapter spawns process
   stdout line ─► parse ─► Event::{TrainStep|AlertRaised|TrainBerthed|TrainFailed|SubagentSpawned}
   owner clicks an option in an alert ─► Command::Answer{alert, answer} ─► adapter writes to stdin
                                                                            (claude) / runs `exec resume` (codex)
   step_complete signal (§7.9) ─► Checking ─► checks pass ─► Done, or AwaitingSignoff if flagged
                                           └► checks fail ─► failure output sent back to the train
   station Done ─► engine readies dependants ─► their worktrees branch off this branch ─► next launches
   process exits ≠0 / error event ─► train Failed (red), station Failed, dependants stay Pending
   process exits 0 without step_complete ─► station stays Active + "stopped without declaring complete" alert
```

#### 8.2.4 Take over (owner requirement)

Select a train, then press `T` or click "Take over" in its inset.

1. pelhaim ends its own control of the session.
   - Claude: close stdin and let the process finish the current turn. A "force" option sends SIGINT.
   - Codex: wait for the exec to finish, or SIGINT it.
2. It opens a terminal running the tool's resume command in the project directory: `claude --resume <session-id>` (verified `-r, --resume`) or `codex resume <id>` (verified subcommand). It uses the `takeover_terminal` argv template.
   - On macOS the default template is `open -a <Terminal|iTerm>` with a small generated `.command` script. The exact mechanism is [Open] Q8.
3. The train becomes **Detached** (magenta), and its adapter is swapped to the **observe** adapter on the same JSONL. pelhaim keeps watching but no longer answers.
4. "Hand back": once the terminal session is idle, pelhaim can re-adopt it by resuming in `-p` mode with `--resume <id>`. This is v2.

Why: only one writer may drive a session at a time. Making the switch explicit and visible (magenta) avoids two processes appending to one transcript. **[Agent's call]**

### 8.3 Direct mode

Here pelhaim *is* the runtime. Its capabilities are `can_answer`, `can_spawn` and `can_take_over = false`; there is no external session to open, but the inset shows the full live transcript.

```
goal ─► Planner train (provider call, structured output: Plan JSON) ─► Plan(Draft) ─► PlanReview alert
owner edits / approves ─► Plan(Approved) ─► for each Ready station: new Train{Direct}
Train loop:
   build ChatRequest(system, history, tools) ─► provider.stream()
   ├─ text deltas ─► inset transcript
   ├─ tool_use ─► risk = tool.risk(input) ─► policy(risk):
   │      allow ─► execute ─► tool_result
   │      ask   ─► AlertRaised(Permission) ─► owner Yes/No/Always-for-this-train ─► execute or deny-result
   │      deny  ─► tool_result{is_error, "denied by policy"}
   ├─ tool `ask_user`   ─► AlertRaised(Question) ─► answer becomes tool_result
   ├─ tool `spawn_agent`─► Fork{Subagent} + child Train on a branch track; parent gets the child's final text as tool_result
   ├─ tool `step_complete` ─► Checking ─► Done / AwaitingSignoff / back to the loop with failures (§7.9)
   └─ stop_reason end_turn without step_complete ─► station stays Active + "stopped without declaring complete" alert
limits per train: max_turns, max_tokens, max_cost (if provider reports usage) ─► exceeded = Failed("limit")
```

- **Providers** (plumbing, **[Owner]** that it's plumbing):
  - `anthropic`: the Messages API over SSE streaming, with tool use.
  - `openai_compat`: `/v1/chat/completions` with `stream: true` and `tools`. This covers Ollama, LM Studio and MLX server.
  - Both normalise to `ProviderEvent::{TextDelta, ToolCall{id,name,args}, Usage, Stop(reason)}`.
  - Don't hard-code model ids. The owner sets them in config.
  - Check Anthropic API details (headers, versions, current models) against current docs at build time. Load the `claude-api` skill.
- **Local models with weak tool calling:** `ModelCaps.tool_calling = false` switches to a text protocol: a JSON block in fenced output, parsed and validated. How well this works is **[Open]** Q9.
- **Built-in tools (v1):**
  - `read_file`, `list_dir`, `glob`, `grep`: risk ReadOnly.
  - `write_file`, `edit_file`: risk WorktreeWrite, or OutsideWorktree / Secrets by path (§10.1).
  - `run`: risk WorktreeExec, or Network / OutsideWorktree / Secrets / Destructive by the §10.1 classifier; an argv, not a shell string, by default.
  - `git_commit` (risk GitTrainBranch): commits on the train's own branch only. No tool can push, merge, or touch `main` (§7.8).
  - `ask_user`, `spawn_agent`, `step_complete`, `report`.
  - No network tool in v1.
- **The orchestration is the core, and is written code:** the plan DAG, scheduling, the approval gate, budgets, and subagent lifecycle and result routing. The model only fills in plans and does the work inside a station.

### 8.4 Mixed modes

Modes can run at once, e.g. observe all Claude/Codex sessions while launching new ones.

- A launched session is also visible to the observe adapter, because it writes JSONL.
- The engine dedups by session id: launch owns the train, and observe ignores session ids registered by launch.

### 8.5 pelhaim as the orchestrator (M2 onward)

**[Owner], 2026-11-23:** run alongside first, replace later.

- **M1:** pelhaim only observes the owner's Claude Code orchestrator. That session sits on the DISPATCH line (§7.7). pelhaim neither plans nor dispatches.
- **M2 onward:** pelhaim can itself be the orchestrator, in Launch or Direct mode. Both can run at once: the owner chooses per goal which orchestrator handles it.

**Routing rules the pelhaim planner must follow** (owner rules; enforcement mechanics are the **[Agent's call]**)

These are the owner's global norms, made concrete enough to build in M2. Each is enforced by written code in the engine, not left to the planner model's goodwill. The planner prompt also states them, so plans come out right first time.

1. **Design and security decisions go to the owner as alerts.**
   - The plan schema requires every step to say whether it involves a design choice (architecture, library, approach, data model) or a security-relevant choice (auth, secrets, permissions, sandboxing, exposure). Those steps carry the `decision` or `security` sign-off flag (§7.9).
   - Plan validation rejects a plan where a step's text matches a design or security keyword list but the step lacks the flag. The owner can override per step. The keyword list is a backstop; the schema field is the main path.
   - During a run, a train that hits an unplanned design or security question must call `ask_user` (Direct) or ask its question (Launch). The train prompt says so. The answer arrives as an alert, never as a silent agent default.
   - Exception, per the owner's standing per-project rules (for example "keep working when blocked"): a train may park the question, continue independent work, and record an "agent default, not yet confirmed" decision. That still raises an alert; it just doesn't block. Which projects allow this is a per-project config flag, default off.
2. **Every decision is logged.**
   - Every answered `Question`, `Signoff`, `PlanReview` and `GitApproval` alert creates a `Decision` row automatically, with "Who" set to Owner.
   - Every parked agent default creates a row with "Who" = "agent default, not yet confirmed".
   - The markdown export stays on-click while two writers exist (§7.6). Unexported rows show as a count badge on the console strip.
3. **Nothing is pushed or merged without the owner.**
   - Push and PR creation only happen through a `GitApproval` alert. Merging to main never happens (§7.8).
   - The planner cannot produce a step whose action is "push", "merge" or "deploy to prod". Such a step must be flagged `merge` or `prod`, and pelhaim itself performs only the push or PR part, after approval.
4. **The plan comes first.** No train departs before the owner approves the plan (§5). Approval is itself logged as a decision.

**Planner inputs** (M2)

- The goal.
- The project's per-repo `CLAUDE.md`.
- The project's open decisions from `DECISION-LOG.md`, read-only.
- The routing rules above.
- The plan JSON schema, with `signoff` and `acceptance` required on every step.

**Output:** a plan validated by §7.5 plus the rule checks above, shown as a `PlanReview` alert.

**What stays with the Claude Code orchestrator until replacement:** everything the owner hasn't moved over. pelhaim doesn't need feature parity in M2. Replacement is complete when the owner says so; that is also when the decision log can move to a single writer.

### 8.6 Keep the Mac awake while trains run

**[Owner], 2026-11-23:** keep the Mac awake while any train is active, release it when none are, and show a small indicator. The mechanism is the **[Agent's call]**.

**When the hold is taken**

- pelhaim holds the machine awake while at least one train is:
  - `Working` or `Checking`, in Launch or Direct;
  - or an observed train that is `Working`, when `include_observed = true` (the default). Observed agents run on this Mac too.
- When `[remote].enabled = true`, a train `Waiting` on an answer also holds. Otherwise the Mac could sleep and the phone's answer could never arrive.
- The hold is released 60 s after the last active train stops. The delay avoids flapping between steps.

**Mechanism on macOS:** `caffeinate -i -m -s -w <pelhaim pid>`, held as a child process. It is spawned on the transition from 0 to 1 active trains, and killed on the transition back to 0.

Flags verified 2026-11-23 against `man caffeinate`:

| Flag | Meaning | Used? |
|---|---|---|
| `-i` | prevent idle sleep | yes |
| `-m` | prevent disk idle sleep | yes |
| `-s` | prevent system sleep; "valid only when system is running on AC power" | yes |
| `-w <pid>` | release when that process exits | yes |
| `-d` | keep the display awake | only with `keep_display_awake = true`, default off. Agents don't need the screen, and a lit screen burns power. |
| `-u` | "declare that user is active" | no. The man page says it defaults to a 5-second timeout, so it does nothing useful for a long hold. |

The owner suggested `-dimsu`; the table explains the two flags left out.

Why `caffeinate` rather than an IOKit `IOPMAssertionCreateWithName` call:

- It needs no `unsafe` FFI or IOKit bindings.
- It is trivially testable: the process exists, or it doesn't.
- The owner can see it with `pmset -g assertions`.
- `-w` releases the hold even if pelhaim is killed with SIGKILL. An IOPMAssertion is also released when its process dies, so the two are equal on safety, and `caffeinate` wins on simplicity.
- IOKit stays the fallback if `caffeinate` ever goes away.

**Limits** (inferred from the `-s` wording and standard macOS behaviour; verify on the owner's machine):

- On battery only `-i` and `-m` apply.
- Closing the lid on battery still sleeps the machine.
- The indicator says "on battery: lid-close will sleep" when that applies.

**Linux**

- When `systemd-inhibit` is on PATH, hold `systemd-inhibit --what=sleep:idle --who=pelhaim --why="agents running" sleep infinity` as a child, killed on release. This command is from memory and unverified; `systemd-inhibit` isn't installed on this Mac. Check `--help` on a Linux box before relying on it.
- Otherwise do nothing, and show the indicator as "not held".

**Indicator**

- A small lamp in the console strip's right-hand status area, next to the mode toggles.
- Lit (in `glyph` teal), labelled `AWAKE` with the count of trains holding it.
- Dim when not held.
- Hovering shows the reason and the battery caveat. Clicking it pauses the hold for 1 hour.
- An a11y name is provided.

### 8.7 Remote control from an iPhone: the seam now, the app in M5

**[Owner], 2026-11-23:** a compatible iOS app gets pelhaim alerts as push notifications, can answer them (option buttons, free text), and shows a compact board. M1–M4 build only the seam. The design below is the **[Agent's call]**; open choices are flagged as owner decisions or questions.

#### 8.7.1 The seam (built from M1)

The engine exposes one internal API that the local UI and any future remote both use, so the remote path is exercised from day one:

```rust
pub trait RemoteApi {                     // implemented by the engine handle
    fn subscribe_alerts(&self) -> broadcast::Receiver<AlertEvent>;      // Raised | Updated | Answered | Dismissed
    fn compact_snapshot(&self) -> CompactBoard;                         // lines → trains {number, state, station, label}, open alert ids
    fn answer(&self, a: Answer) -> Result<AnswerOutcome>;               // Answer { alert_id, choice: Options(Vec<idx>) | Text(String), origin }
    fn dismiss(&self, alert_id: AlertId, origin: Origin) -> Result<()>;
}
pub enum Origin { Local, Remote { device_id: DeviceId } }
```

- **Answers are idempotent, and the first one wins.** A late answer to an alert already answered elsewhere returns `AlreadyAnswered { by, at }`, and the phone shows "answered on Mac".
- **Every decision row records its origin** (Mac or which device).
- **Each alert kind declares `remote_answerable`.** Defaults:
  - Question, Permission, Signoff, AssignProject: answerable;
  - GitApproval (push or PR) and anything flagged `security`: view-only from the phone unless `[remote].allow_high_risk = true`, and even then only after Face ID in the app. This is an owner decision; see Q18.
- **`CompactBoard` is a small, versioned, serde type** with no transcript text: just line names, train numbers, states and alert summaries. Alert bodies travel only inside answerable alerts, and only encrypted.
- **M1–M4 deliverable:** the trait, the `Origin` field, idempotent answers, `CompactBoard`, plus tests that drive answers through `RemoteApi` with `Origin::Remote`. No network code.

#### 8.7.2 Architecture (M5)

```
pelhaim (Mac) ──outbound TLS WebSocket──► relay ◄──outbound TLS WebSocket── iOS app (foreground)
                                            │
                                            └──► APNs ──► iPhone (background push; Notification Service Extension decrypts)
```

- **No open ports, by default and in every configuration.** pelhaim and the phone both connect *out* to the relay. Nothing listens on the Mac.
- **The relay** (`pelhaim-relay`, Rust/axum) is deliberately dumb:
  - it routes opaque encrypted envelopes between paired ids;
  - it queues up to N envelopes per device for 24 h while the phone is offline;
  - it sends APNs pushes.
  - It holds no keys that can decrypt anything. It sees routing ids, sizes, timing and APNs device tokens. That metadata is stated plainly in the README.
- **Push:** the APNs payload carries `mutable-content: 1` and only the encrypted envelope. The app's Notification Service Extension decrypts it on the device and writes the visible text ("1148 · SafeAscent: Proceed to C1?"). Apple sees ciphertext only.
  - The notification offers a text-reply action and an "Open" action.
  - Option buttons are shown in the app, or as notification actions if a fixed category set proves workable; see Q19.
  - This mechanism (`mutable-content`, service extension, text-input action) is standard iOS from memory. Verify it against Apple's current docs at M5.

#### 8.7.3 Security

- **Identities:**
  - pelhaim has a long-term X25519 key (encryption) and an Ed25519 key (signing), stored in the macOS Keychain.
  - The phone generates its own pair in the Secure Enclave or Keychain.
  - No key ever leaves its device.
- **Pairing (QR plus a key exchange):**
  1. The owner runs `pelhaim remote pair`, or uses the board's Devices panel.
  2. The board shows a QR code holding: the relay URL, pelhaim's public keys, a fresh 32-byte one-time pairing secret, and an expiry (5 minutes).
  3. The app scans it and sends its public keys to pelhaim through the relay. The message is authenticated with a MAC keyed from the pairing secret, so only someone who saw the QR can pair.
  4. Both sides show a 6-digit code derived from both public keys. The owner confirms that the codes match on the Mac. This blocks a relay that swaps keys.
  5. The device is stored (id, keys, name, paired-at). The pairing secret is burned.
- **End-to-end encryption:**
  - Every envelope is sealed for one recipient with **HPKE in Auth mode** (RFC 9180): DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305.
  - Auth mode proves the sender's identity key, and a fresh ephemeral key per message gives per-message forward secrecy for the sender's side.
  - HPKE fits the store-and-forward relay, because no live handshake is needed.
  - Rust: the `hpke` crate. iOS: CryptoKit's HPKE (iOS 17+, from memory; verify at M5).
- **Auth to the relay:**
  - Each party proves its device id by signing a relay challenge with its Ed25519 key.
  - The relay only forwards between ids registered as a pair. It rate-limits per id.
  - Pairing registration on the relay requires a proof derived from the pairing secret.
- **Replay protection:** each envelope's authenticated data carries sender id, recipient id, a monotonic 64-bit sequence number and a timestamp.
  - The receiver rejects a sequence number at or below the last one it accepted from that peer, and anything older than 10 minutes.
  - Answers also carry the alert id, so an answer is only accepted while its alert is open (idempotency, §8.7.1).
- **Revocation and loss:**
  - The Devices panel on the Mac revokes a device immediately: it deletes the key locally and deregisters it at the relay.
  - The app can require Face ID to open. Default on; owner decision Q18.
- **Least privilege:** the remote can do only what `RemoteApi` exposes: answer, dismiss and read `CompactBoard`.
  - It cannot launch, plan, change config, read transcripts, or approve anything marked view-only.
- **The relay holds no secrets** beyond the APNs signing key (.p8) and its own TLS certificate. Both live in the host's secret store; never in the repo, with `.env.example` placeholders only.

#### 8.7.4 Owner decisions and cost (flagged)

- **Apple Developer Program:** needed for APNs and for installing the app on a phone beyond short-lived development builds. Cost: **$99 per year** (Apple's long-standing price; re-check). **Owner decision.**
- **Relay hosting:** a small always-on service. Railway (one of the owner's usual hosts) or any small VM; likely a few dollars a month (unverified; check current pricing). Vercel functions are a poor fit for long-lived WebSockets. **Owner decision.** See Q15.
- **Use an existing push service instead of our own relay?** See Q16.
  - **ntfy:** self-hostable, has an iOS app. But answers would have to come back another way, and it has no E2E by default.
  - **Pushover:** one-off paid app; notifications only, no answering.
  - **Tailscale:** no public relay at all; the phone reaches the Mac over the tailnet. But the Mac would have to listen on its tailnet IP, and you still need APNs for background pushes.

  Recommendation: our own relay. Answering plus E2E is the point, and none of the off-the-shelf options gives both.
- **Native app stack:** SwiftUI app plus a Notification Service Extension, in a separate repo `pelhaim-ios`, public, MIT. The alternative is a cross-platform stack (React Native, Flutter); rejected because the push extension and CryptoKit are native-first. **Owner decision.** See Q17.

## 9. UI spec

### 9.1 Visual system

**[Owner]** for direction; specific values are **[Agent's call]** and should be tuned by eye.

**Units**

- One unit system: "board units", the spike's 960-basis pixels from VISUAL-SPEC.md.
- A view has `k` (zoom) and an `(ox, oy)` offset. Everything is drawn through it.
- The zoom inset is the *same scene* drawn with a second view, clipped (spike lesson).

**Palette**

- Start from the measured film values. Convert them to OKLab, multiply chroma by 0.6 and lightness by 0.88, and keep the hue.
- This gives the matte board-lamp look the owner asked for. Computed today:

| Token | Film (measured) | Matte start value | Use |
|---|---|---|---|
| `bg` | `#0D0B21` | keep `#0D0B21` | canvas: dark indigo, not black |
| `idle` | `#A9DADD` | `#99B5B7` | idle blocks, platform bars |
| `work` | `#75D068` | `#76A96F` | working train and occupied blocks |
| `route` | (none) | `work` with lightness ×0.72 and chroma ×0.8 = `#466C41` | cleared route ahead |
| `wait` | `#F7F06E` | `#CCC984` | waiting on owner |
| `fail` | `#FD6160` | `#BF6661` | failed / blocked |
| `fork` | `#D868F5` | `#A86AB8` | forked / detached |
| `done` | `#85889A` | **keep `#85889A`** (owner-specified) | done blocks and trains |
| `label` | `#D5D6DA` | `#B4B4B7` | station and line labels |
| `glyph` | `#74A6A9` | `#6D898A` | track ends, gates |
| `frame_navy` | `#0A1837` | keep | popup body band |
| `frame_dash` | `#4FA8C8` | `#5B8A9C` | popup dashed border |

- `done` (L≈0.63, blue-grey) and `idle` (L≈0.76, cyan) differ in both lightness and hue, so they read as distinct.
- `palette = "film"` gives the unmuted values; `high_contrast` is for accessibility.

**Strokes and blocks**

- Keep the block-segment language: every track is a chain of blocks with gaps, as in the film's track circuits.
- Draw blocks as anti-aliased rounded-end-free rectangles. Keep the square ends, but anti-alias the edges in the shader (an SDF quad). That is the "smoother, not blocky" ask, without drifting to rounded pill shapes.
- Block rhythm should be calmer than the spike:
  - fewer very short blocks;
  - a block length of 14–24 units;
  - a gap of 4 units;
  - tracks 2.5 units thick, which is thinner than the spike's 3.

**Density cut vs the spike**

- At most **4 visible tracks per line**. More collapse into a "+N" stub that expands on click.
- **No decorative crossovers.** An "X" appears only where a subagent's result actually merges back.
- More space between bundles: 36 units instead of 25–30.
- Station labels only at real stations. No header row unless there are milestones.
- Lines with no activity in `active_window` fold to a single thin dim track with the line label.

**Film elements kept:**

- train tags: a filled rectangle with dark digits, a direction arrowhead, and a dotted underline;
- platform bars that light when a train berths;
- sharp ~60° diverging tracks;
- teal end glyphs;
- the yellow selection rectangle;
- the offset frame-stack inset.

The dotted underline under a tag is a **context-use meter**: dots fill as the session's context fills, when that is known. **[Agent's call]**

**Motion**

- Trains **step** block to block. There is no sliding; this matches the ATS boards the visual spec infers.
- A waiting train's tag pulses slowly, about 1 Hz, between `wait` and a dimmer `wait`. Nothing else animates by itself.
- `motion = "reduced"` turns off the pulse and the inset zoom tween.
- An optional "on-set" post pass (gentle bloom, slight black lift; VISUAL-SPEC §10) is off by default, given the owner's matte direction. It is an M4 nicety.

**Font**

- An original pixel font, designed in-repo as text grids, e.g. `assets/font/glyphs.txt` with `#`/`.` rows per glyph. A build script compiles it into glyph data. This keeps the font diffable and reviewable by agents.
- Two cuts:
  - **5×7 cell** for tags and labels;
  - **7×9** for line names, popups and the console.
- Features, from VISUAL-SPEC §5: squarish rounded bowls, uniform stroke, the hooked "1" (foot to the right), and mixed case.
- **Rendering:** each glyph pixel is a quad.
  - At tag sizes, snap to whole device pixels and draw without AA, so it stays sharp.
  - In the zoom inset, quads scale with `k`, so text stays crisp at any zoom with no atlas resampling.
- The glyphs are drawn fresh, not traced from film frames. Share Tech Mono (OFL) is used only as a visual comparison during design; it doesn't ship.
- **Text scope:** a Latin-1 subset in v1.
  - Characters outside it render with a fallback system font through the glyph path.
  - Whether to bundle a fallback font for non-Latin text from transcripts is [Open] Q10.

### 9.2 Renderer crate choice

**[Agent's call]**

**Pick: `winit` + `wgpu` + a small in-house batched renderer** (quads, SDF-AA lines and triangles, scissor), plus **AccessKit** (`accesskit_winit`) for accessibility.

| Criterion | in-house on wgpu + winit | macroquad (spike) | egui/eframe | iced |
|---|---|---|---|---|
| Fit to the board look | full control | full control | fights it: widget look, needs heavy restyling | fights it |
| Text input and IME | winit delivers `Ime` events (commit, preedit) and pelhaim builds one single-line and one multi-line input widget | no IME support; key-by-key only | built in | built in |
| Popups / multi-window | popups are in-scene overlays (better for the look anyway); winit can open extra windows for a later "wall mode" | single window | single + viewports | multi-window |
| Accessibility | AccessKit: pelhaim publishes a tree for trains, alerts, buttons and input | none | AccessKit built in | partial |
| Testability | the display list is pure data; offscreen wgpu render to texture for pixel goldens | global-state API, awkward headless | good | moderate |
| Cost | the most code: about 1–2k lines for renderer, input widget and a11y bridge | the least | low, but restyling cost is high | medium |

Why:

- The board is 95% custom drawing, and the remaining 5% (text input, buttons) is small.
- macroquad was right for a throwaway spike. Its lack of IME, AccessKit and multi-window, plus its global state, rule it out for a daily tool.
- egui is the runner-up. If text input becomes a time sink, an acceptable fallback is to run egui **only for the console input**, painted into the same wgpu pass with a custom-styled theme.
- Don't use `vello` or `lyon`: SDF quads cover every primitive the board needs.

### 9.3 Screen layout

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ DISPATCH · 6 agents running                                                              │  ← one-row strip, no train (§5.3)
│ SAFEASCENT ●2      plan ─── migrate 0007 ─── backfill ─── Stage C1 ARR 14:52              │  ← line label (+ open-decision badge), stations, arrival clock
│  ━━ ━━ ━━ ━[1423▶]━━ ━━ ━━ ━━ ━━ ━━ ━━ ━━                                                │  ← tracks, blocks, tags
│  ━━ ━━ ━━ ━━ ━━ ━━ ━━[◀1148]━━ ━━ ╲                                                      │
│                                      ╲━━ ━━ ━━ (magenta branch: detached)                │
│ ETHICA            scenarios ─── corpus rev ─── judge val …                               │
│  …                                                                                       │
│                                                                    ┌ alert tray ┐        │
│                                                                    │ ? 1148  2m │        │
│                                                                    │ ✓ 0942 res │        │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ DISPATCH ▸ _                                                       [obs][launch][direct]  │  ← console strip
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

**Canvas and navigation** (inspired by the ~9:1 wall; VISUAL-SPEC §1)

- The board is a horizontally scrolling wide canvas.
- Navigation: scroll or drag to pan; pinch or ⌘± to zoom; `0` fits all; `F` fits the selection.
- Lines stack vertically, in config `order`. Lines with open alerts float to the top. This is a config toggle, default on.

**Interactions**

| Input | Action |
|---|---|
| Click a tag | Select it: yellow outline. Show a one-line status in the console strip. |
| Double-click a tag, or `Enter` | Open the zoom inset on that train. Inside the inset: the train's live transcript tail, its station list, and actions (Take over, Stop, Open cwd, Copy session id). |
| Click a yellow tag | Open its alert. |
| `Tab` / `Shift-Tab` | Cycle trains with open alerts, oldest first. |
| `J` / `K` | Next / previous train. |
| `Esc` | Close the topmost popup or inset. |
| `Space` | Pause board motion (state still updates). |

Keyboard-only use must be complete.

### 9.4 Alerts

**[Owner]** for the style; layout is **[Agent's call]**.

**Frame** (from the film's Maintenance window and zoom frame stack)

- From the outside in:
  - an outer thin white line;
  - a thin yellow line, offset;
  - a navy band with a cyan dashed border;
  - a dark body.
- A light tab at top-left carries the alert kind: `Question`, `Permission`, `Result`, `Failure`, `Plan`, `Stalled`.
- In the matte palette, the white and yellow lines are drawn at reduced lightness so the frame doesn't shout.

**Body**

- Line 1: the train number and line, e.g. `1148 · SafeAscent / Stage C1`.
- Line 2: `STATUS: WAITING ON YOU`, in the kind colour.
- Then the question text, wrapped.
- **Options as checkbox rows:** `[1] ☐ Proceed to C1 rehearsal — <description>`.
  - Single-select acts on click.
  - Multi-select (`multiSelect: true`) toggles rows, then a `[Send]` button.
  - The last row is always `[0] Other…`, which opens an inline text field. The answer box is also free text.
- **Buttons:** `[Send]`; `[Log decision]` (after answering); ✕ at top right.

**Behaviour**

- **One-click answer** with the mouse, or number keys `1–9` then `Enter`.
- **✕ / Esc dismisses to the tray.** It does not delete. The tray, bottom-right, lists open items, newest first, with age. Clicking one reopens it.
- **Placement:** anchored next to the train's tag, flipping above or below to stay on screen, like the spike. The connector line runs to the tag.
- **Stacking:** at most **one popup open at a time**. Others queue in the tray, with a count badge on the console strip. This prevents a wall of modals.
- **Observed sessions:** options are shown read-only, with `Answer in terminal` and `Copy session id`. When the answer shows up in the log, the alert closes itself and offers `[Log decision]`.
- **Results and failures:** a `Result` alert shows the train's final message summarised as a checklist (§9.5), with `[Open transcript]` and `[Log decision]`. `Failure` adds `[Retry station]` (Launch/Direct only).
- **Plan review:** a larger popup with the step list as checkbox rows. Unchecking a step skips it.
  - `[Approve & depart]`, `[Edit]` (§9.6) and `[Discard]`.
  - Behind the popup, the board shows the draft plan's stations in dim cyan.
  - Each step row shows its sign-off flags and acceptance checks, which can be edited before approval.
- **Sign-off** (§7.9): the tab reads `Sign-off`. The body has the completion summary, a check-result checklist (`☑ cargo test --workspace`, `✕ …`), the diffstat, and `[Sign off]`, `[Reject with note]`, `[Log decision]`.
- **Git approval** (§7.8): the tab reads `Push` or `Pull request`. The body has branch → base, commit list, diffstat, and `[Approve]` / `[Not now]`. ✕ means "not now"; it never means approve.
- **Merge conflict** (§7.8): a red `Conflict` tab, the file list, `[Open worktree in terminal]` and `[Retry merge]`. On the board, a red X joins the two tracks.
- **Assign project** (§7.7): for Unsorted trains. Options are the configured projects plus "this is the orchestrator". The choice is remembered per session.

### 9.5 Console strip and chat styling

**[Agent's call]**, within the **[Owner]** checkbox / notification / alert direction.

- A single-row strip along the bottom, styled as a control-desk terminal: a navy band with a cyan dashed top border.
- The prompt `DISPATCH ▸` is in the 7×9 font. Typing grows it to at most 6 rows. `⌘↑` expands the **radio log** above it, a scrollback of the dispatcher conversation.
- **What you type goes to the dispatcher:**
  - Launch or Direct: the planning agent.
  - Observe-only: the dispatcher is pelhaim's own command parser, with commands like `/goto ethica`, `/decisions`, `/mode`.
  - `@1148 …` addresses a specific train (answers it, or sends a follow-up).
- **Dispatcher and agent replies are never rendered as free prose blocks.** They render as notice cards:
  - `☑ done item`, `☐ pending item`, `⚠ needs you`, `✕ failed`, each in the state colour;
  - a card has a kind tab like the alerts, but small.
- Agent text is parsed into cards by simple written rules:
  - markdown list items become rows;
  - `- [x]` and `- [ ]` become checkboxes;
  - lines starting with "Error" or "Failed" are ✕;
  - anything else is a plain row.
- Full raw text is one click away (`[raw]`).

### 9.6 Plan editing

**[Agent's call]**

- **v1, two ways:**
  1. **On the board:**
     - select a draft station and press `E` to rename it or edit its detail;
     - `Del` to remove it;
     - drag from one station to another to add a dependency;
     - `A` to add a step after the selection.
  2. **`[Edit]` opens the plan as TOML in `$EDITOR`.** On save, pelhaim re-reads it, validates it (§7.5), and redraws. Validation errors show as a `Failure` alert listing the line numbers.
- Path 2 is cheap and robust, and it suits an owner who lives in a terminal. Path 1 is nice to have, and M2 can ship with path 2 only.
- Plan revisions are kept (`Plan.revision`). The approved revision is what's dispatched.

### 9.7 Accessibility

- AccessKit tree: each line is a group, and each train is a node with name `Train 1148, SafeAscent, waiting on you: <question>`. Alerts are dialogs; options are checkboxes or radio buttons.
- State is never colour-only:
  - tags carry a 1-glyph state mark in the inset and the a11y name;
  - `high_contrast` palette;
  - focus is always visible (the yellow outline).
- `motion = "reduced"` is honoured.

## 10. Security

All of §10 is **[Agent's call]**, from the owner's norms. The owner should look at it.

- **Keys:**
  - Read keys from an env var, or from the Keychain via the `keyring` crate, named in config as `{ env = … }` or `{ keychain = … }`.
  - Hold them in `secrecy::SecretString`, whose `Debug` and `Display` are redacted.
  - The HTTP client's logging layer strips `authorization` and `x-api-key`.
  - `pelhaim doctor` reports "present / missing" only.
  - `.env` is gitignored; `.env.example` lists `ANTHROPIC_API_KEY=` with a dummy value.
- **Observe is read-only:**
  - Files are opened with read-only handles.
  - The observe crate has no write paths.
  - A test runs the observe adapters over a fixture home and asserts no file in it changed: mtime, size and hash are unchanged, and no new files appear.
- **Direct-mode tools:**
  - *Confinement:* every path is canonicalised and must sit under the train's workspace root. Symlinks that escape are refused. The `run` tool gets argv, not a shell string, unless policy allows `sh -c`.
  - *Approval gate:* there is a policy per risk class (config), with **auto mode as the default** (**[Owner], 2026-11-23**). See §10.1 for the classes, defaults and stop points. "Always allow for this train" is session-scoped and never persisted silently.
  - *Isolation:* each train runs in its **own git worktree** on its own branch, so every change is reviewable and can be discarded. **[Owner], 2026-11-23**: this applies to Launch mode too (§7.8).
  - *Process limits:* timeout, output cap, scrubbed environment. The child does not inherit `ANTHROPIC_API_KEY` or other secrets; there is an explicit allowlist.
  - *OS sandbox:* not in v1. macOS `sandbox-exec` is deprecated, and Linux options (Landlock, bubblewrap) need care. Called out as Risk R4.
- **Launch mode:**
  - pelhaim passes the owner's permission policy through to the CLIs.
  - It never adds `--dangerously-skip-permissions` or `--dangerously-bypass-approvals-and-sandbox` on its own. Those flags are exposed only if the owner sets them per project in config, with a red badge on the line label.
- **Git** (**[Owner], 2026-11-23**; §7.8):
  - pelhaim never commits to or merges into `main`.
  - Push and PR creation only happen after an owner-approved alert.
  - Branch, worktree and commit actions go through the `git_train_branch` policy (auto-allowed by default).
  - After every Launch step, pelhaim checks that `main` has not moved, because it cannot mediate the CLI's own git commands.
- **Untrusted content:**
  - Transcript text, tool output and model output are data. They are rendered as text and never interpreted as commands by pelhaim.
  - Links in transcripts open only on an explicit click.
- **Local providers:** a non-loopback `http://` base URL is refused unless `allow_insecure = true` is set for that provider.
- **No telemetry. Logs** (`tracing`, to `<data_dir>/logs`) carry ids, counts and states, never message bodies.
- **CI:**
  - `cargo audit`, or `cargo-deny` with advisories, licences and bans.
  - Dependencies pinned via the committed `Cargo.lock`.
  - Run `/secscan` before the first push.

### 10.1 Auto mode and stop points

**[Owner], 2026-11-23:** auto mode is the default. Agents run on their own, and only key decisions stop them. The risk classes, the classifier and the CLI mapping are the **[Agent's call]**. CLI flags were verified 2026-11-23 against `claude --help` 2.1.331 and `codex --help` / `codex exec --help` 0.158.0.

**Classes and defaults**

| Class | What falls in it | Default |
|---|---|---|
| `read_only` | reads anywhere the agent is allowed to see, except secrets | allow |
| `worktree_write` | create, edit or delete a single file inside the train's own worktree | allow |
| `worktree_exec` | a command with cwd = the worktree that the classifier doesn't put in a class below (builds, tests, linters, formatters) | allow |
| `git_train_branch` | branch, worktree, commit on the train's own branch | allow |
| `outside_worktree` | any write, or any command argument, naming a path outside the worktree (after canonicalising and following symlinks) | **ask** |
| `network` | network-capable commands (`curl`, `wget`, `ssh`, `scp`, `git fetch/pull/push`, package installs, `npm publish` and similar) and any network tool | **ask** |
| `secrets` | reading `.env*`, keychains, `~/.ssh`, `~/.aws`, credential and token files; env vars with key-shaped names | **ask** |
| `destructive` | `git reset --hard`, `git clean`, `git branch -D`, `git push --force`, history rewrites (`rebase`, `filter-*`, `commit --amend` on shared commits), recursive `rm`, `drop`/`truncate` against a database, anything touching a deploy target | **ask** |
| push / open PR | always an owner alert. Not configurable (§7.8). | fixed |
| merge to main | never done by pelhaim | fixed |
| design / security / merge / prod steps | stop for sign-off after checks (§7.9). Not configurable. | fixed |

- **Per-project overrides:** `[project_overrides.<slug>.approval]` uses the same keys (**[Owner]**: overrides stay).
  - An override can tighten anything.
  - It can loosen any class except the fixed rows.
  - Loosening `destructive`, `secrets` or `outside_worktree` draws a red badge on the line label.
- **Classifier (Direct mode):**
  - Written code over the tool input: canonicalised paths compared with the worktree root, and argv matched against the lists above.
  - When unsure, it picks the stricter class.
  - Without an OS sandbox it is a best-effort classifier, not a guarantee. R4 records this.
- **How auto mode fits §7.9:** auto mode governs the tool calls *inside* a station. The definition of done is unchanged.
  - A station still ends with `step_complete` plus checks run by pelhaim.
  - Flagged stations still stop for sign-off.
  - So a typical station produces zero alerts until the end, and an alert only at a real stop point. That matches how the owner's sessions run today.
- **Routing rules (§8.5) still apply on top:** an unplanned design or security question is a stop point whatever the policy says.

**Mapping onto the CLIs (Launch mode)**

- **Claude Code** (`claude -p`):
  - `--permission-mode auto`. Verified as a choice; the full list is `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`.
    - The exact semantics of `auto` aren't documented in `--help`. The design assumes it is the classifier-backed auto mode the owner already uses interactively, which allows routine work and escalates risky actions. The M2 spike must confirm this.
    - If it isn't, fall back to `acceptEdits` plus `--allowedTools` for in-worktree commands.
  - `--permission-prompts host` (verified; the default) sends whatever the CLI does escalate to pelhaim, which raises it as an alert.
  - `--disallowedTools` (verified; pattern syntax like `"Bash(git *)"`) hard-blocks the fixed stop points the CLI shouldn't attempt at all, e.g. `"Bash(git push *)"` and `"Bash(gh pr merge *)"`. pelhaim performs push and PR itself after the owner approves.
  - The cwd is the train's worktree, and no `--add-dir` is passed by default, so the CLI's own file scope is the worktree.
  - `bypassPermissions` and `--dangerously-skip-permissions` are never used unless the owner sets them per project (§10).
- **Codex** (`codex exec`):
  - `-s workspace-write` (verified) gives Codex's own sandbox, scoped to the workspace = the worktree.
  - `--approve-for-me` (verified: "Route approval requests through automatic review using the workspace-write sandbox") is the closest match to auto mode.
  - `-a, --ask-for-approval <on-request|never>` is verified only on top-level `codex`. It is **not listed in `codex exec --help`**, so don't pass it to `exec`. A `-c` config-key equivalent is unverified.
  - How the sandbox treats network access by default is unverified. Check it in the M2 spike, and add `-c` overrides only after confirming the key names.
  - `--dangerously-bypass-approvals-and-sandbox` is never used unless set per project.
- **What the CLIs can't express:** pelhaim enforces it after the fact instead:
  - the post-step checks that `main` and remotes haven't moved (§7.8);
  - a diff scan of the worktree for files written outside expected paths;
  - a red alert on any violation.

## 11. Testing strategy

**[Agent's call]**

| Layer | How | What |
|---|---|---|
| `pelhaim-core` | Plain unit tests and `proptest` | Reducer: fixture event sequences lead to expected state. Plan validation: cycles, missing deps, duplicates. Path cover and layering. Train movement rules: never passes an unfinished station. Train numbering collisions. |
| Adapters, observe | Synthetic fixture homes under `tests/fixtures/{claude,codex}/…` written by hand to match today's shapes, **never real logs** | Parsing every record type listed in §8.1; partial last line; invalid JSON lines; subagent meta linking; `AskUserQuestion` open/close; stall and permission heuristics on a fake clock; the read-only assertion. **Project tags (§7.7), each for Claude subagent, Claude self-tag and Codex:** tagged (lands on its line); untagged (Unsorted + AssignProject alert); mis-tagged (unknown slug, malformed key such as `Pelhaim-Project:`, tag on line 2, tag inside a code fence); a DISPATCH session with tagged subagents branching to two different lines; a cwd of `~/Developer` that must *not* influence the line; an owner assignment surviving a restart. **Repositories (§7.7 step 4, 2026-11-24), on synthetic temp trees:** a plain repo; a linked worktree resolving to the main repo's name; nested repos; no repo (Unsorted); a tag overriding the repo; auto-created lines; a configured `path` match; a symlinked cwd; and the read-only proof over the repo tree. |
| Adapters, launch | Fake `claude` and `codex` executables (small Rust test binaries) placed first on `PATH`, replaying recorded stream lines, reading stdin, and exiting with set codes | Spawn args are exactly what §8.2 says, including the tag line and the worktree cwd; answer routing; exit-code handling; take-over handoff; `step_complete` via MCP tool and via final-line JSON; look-alike JSON in a code fence ignored; a turn ending without the signal. Fixture streams come from the M2 spike recordings, scrubbed. |
| Git flow | Throwaway repos built in temp dirs with `git` | Worktree + branch per train; a dependant branches off its predecessor's branch; a two-predecessor merge, clean and conflicting (red X + alert); no code path can commit to or merge into `main`; push and PR are only reachable through an approved `GitApproval`; "main moved" detection. |
| Done / sign-off | Engine plus fake checks | Declare → Checking → Done; a failing check returns to Active, then Failed after `max_check_attempts`; flagged steps stop at AwaitingSignoff and don't release dependants; reject-with-note; plan validation rejects design/security steps without flags. |
| Providers | `wiremock` HTTP server | Anthropic SSE and OpenAI-compatible streaming, tool-call assembly from deltas, errors, retries, key never in logs (assert on captured logs). |
| Runtime | A scripted fake `Provider` | Approval gate per risk class; path confinement (escape attempts); budgets; `spawn_agent` / `ask_user` round trips; worktree creation. |
| Auto mode (§10.1) | Table-driven classifier tests | Every row of the class table, with positive and negative cases (`../` escapes, symlinks out of the worktree, `git push` hidden in `sh -c`, `.env` reads, `rm -r`); a stricter class always wins; per-project overrides tighten and loosen, but never the fixed rows; the Launch spawn args carry the §10.1 mapping exactly. |
| Keep awake (§8.6) | A fake `caffeinate` / `systemd-inhibit` on `PATH` that records its argv and lifetime | Held exactly while ≥1 train is active; the 60 s release delay; `-w <pid>` passed; `-d` only when configured; observed trains count when `include_observed`; a no-op path when neither tool is present. |
| Remote seam (§8.7.1) | Engine plus `RemoteApi` | Answers with `Origin::Remote`; first answer wins and the later one gets `AlreadyAnswered`; view-only kinds refuse remote answers; `CompactBoard` contains no transcript text (asserted). M5 adds: pairing with the code check, HPKE round trip, replay and old-timestamp rejection, a relay that never sees plaintext (a test relay asserting envelopes are opaque), and revocation. |
| Store | Temp dirs | Event replay equals projections. Decision-log writer: inserts under the right heading, new section creation, concurrent-modification retry, byte-identical preservation of the rest of the file (golden files). |
| Board (scene) | `insta` snapshots of the `DisplayList`, serialised as compact RON | Fixed fixture states lead to identical display lists. This catches layout and colour regressions without a GPU. |
| GPU renderer | Offscreen render to texture, then compare PNG to a golden with a per-pixel tolerance | Run locally on macOS (Metal), and on Linux CI with Mesa `lavapipe`. Keep a small set of goldens: board, alert, inset, console. If lavapipe proves flaky, CI runs scene snapshots only and pixel goldens stay a local `cargo test --features gpu-goldens`. |

One command: `cargo test --workspace`. Goldens are updated with `cargo insta review` and `PELHAIM_BLESS=1`.

## 12. Repo and CI

**[Agent's call]**, implementing **[Owner]** norms.

**First code commit contains:**

- `README.md` (what pelhaim is, a screenshot once one exists, install and run, the three modes, the film-inspiration note);
- `LICENSE` (MIT);
- `CLAUDE.md` (build, test and run commands; the PATH note for `~/.cargo/bin`; conventions; the "never commit reference images" rule);
- `.env.example`, `.gitignore` (`target/`, `.env`, `*.db`, `*.pelhaim-bak`);
- `CHANGELOG.md`, `rust-toolchain.toml`;
- `.github/workflows/ci.yml`.

**CI** runs on every PR and on push to `main`, on `ubuntu-latest` and `macos-latest`:

- `cargo fmt --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- `cargo deny check`.

**Release / "deploy" job:** on a `v*` tag, build release binaries for macOS (arm64) and Linux (x86_64) and attach them to a GitHub Release. It is drafted, not published automatically; publishing is the owner's call. `cargo-dist` is a reasonable tool for this. **[Open]**

**The film** appears in the README only as "inspired by the control-room board in a 2009 film". No title in the binary, no assets, no logos. Mentioning the title in the README text is fine; this doc does. Product name: **pelhaim**. **[Owner]**

## 13. MVP milestone cut

**[Agent's call]**. This follows the owner's suggested order, with acceptance criteria added.

**M1: Observe + board + alerts** (the "useful on day one" milestone)

- Workspace, CI, README, LICENSE, CLAUDE.md, config, `pelhaim doctor`.
- Font v1 (5×7 + 7×9, hooked "1"), matte palette, renderer, board layout, zoom inset, selection, pan and zoom.
- `observe::claude` (including subagents) and `observe::codex`, with tailing, state derivation and the stall and permission heuristics.
- Alerts:
  - Question (read-only), Result (opt-in), Stalled;
  - tray, keyboard navigation;
  - `[Log decision]` writer, plus the open-decisions badge from `DECISION-LOG.md`.
- Project tags (§7.7): the DISPATCH line, the Unsorted line, the AssignProject alert. Plus the one-line addition to the owner's orchestrator rules so it tags every agent it spawns (the edit goes in the setup repo, done by the owner or with the owner's approval).
- pelhaim only observes the Claude Code orchestrator in M1 (§8.5).
- Console strip with local commands only.
- AccessKit basics.
- Keep-awake (§8.6) with its indicator. Observed working trains hold the Mac awake.
- The `RemoteApi` seam (§8.7.1): the trait, `Origin`, idempotent answers, `CompactBoard`. The local UI answers through it. No network code.
- **Done when:**
  - with the owner's real 7 projects configured, every **tagged** Claude Code and Codex session and agent shows as a train on the right line, and untagged ones land on Unsorted with an assign alert (**[Owner], 2026-11-23**);
  - the orchestrator session shows on the DISPATCH line, with its subagents branching onto their lines;
  - an `AskUserQuestion` in any session raises a yellow train and an alert within 2 s;
  - answering in the terminal closes it;
  - a decision can be logged into `DECISION-LOG.md` without disturbing the rest of the file;
  - CI is green on both OSes.

**M2: Launch mode + planning**

1. A **spike**: record real `claude -p` stream-json and `codex exec --json` sessions, including a permission prompt, a question and a subagent. Settle [Open] Q6 and Q7, and commit scrubbed fixtures.
2. `launch::claude`, `launch::codex`; answering; take over and detach; dedup with observe.
3. Dispatcher planning via a launched session with `--json-schema` / `--output-schema` to produce Plan JSON. Plan review alert, `$EDITOR` editing, approval, the scheduler, and cleared-route rendering.
4. pelhaim as orchestrator: the routing rules of §8.5, with automatic decision rows.
5. Auto mode for Launch (§10.1): the CLI permission mapping confirmed by the spike, and the post-step violation checks.
6. Git flow (§7.8): worktree and branch per train, stacking, merge conflicts, push/PR approval alerts.
7. Definition of done (§7.9): `step_complete` (MCP tool or final-line JSON), checks, sign-off states and alerts.
8. The console sends to the dispatcher; `@train` messages.
- **Done when:** the owner can type a goal, approve a 4-step plan with one dependency fork, and watch 2 trains run in parallel, each in its own worktree. Along the way they answer one question from the board, take over one train in a terminal, see a dependent station branch off its predecessor's branch, sign off one flagged step, approve one PR (opened, never merged, by pelhaim), and see every decision recorded.

**M3: Direct mode**

- `Provider` trait, `anthropic`, `openai_compat` (tested against a local Ollama or LM Studio).
- Agent loop, tools, the auto-mode approval gate and classifier (§10.1), worktree isolation, budgets, `spawn_agent` / `ask_user`.
- The planner runs on a provider.
- **Done when:** the same M2 acceptance run works with Direct mode on Anthropic, and on one local model, with every tool call classified by the approval gate: in-worktree work auto-approved, and every stop point in §10.1 raising an alert.

**M4: Polish** (optional)

- On-board plan editing (drag dependencies), hand back after take-over, wall mode across multiple windows, the on-set post pass, Windows, a bundled fallback font.

**M5: Remote control from an iPhone** (**[Owner], 2026-11-23**; §8.7)

- Needs the owner decisions in §8.7.4 first: the Apple Developer account, relay hosting, and the app stack.
- `pelhaim-relay` (deployed, with CI and a deploy job before the first production push), `pelhaim-remote` (pairing, HPKE, relay client), and a Devices panel on the board.
- `pelhaim-ios` (SwiftUI app + Notification Service Extension) in its own repo: pairing by QR, an alert list, answering (options and free text), a compact board, Face ID gate.
- **Done when:**
  - the owner pairs their iPhone by QR with a matching code;
  - an alert from a running train arrives as a push within ~5 s;
  - the notification text is readable on the lock screen while the relay and APNs only ever handled ciphertext;
  - answering from the phone unblocks the train and records a decision with origin "iPhone";
  - an answer replayed through the relay is rejected;
  - revoking the device on the Mac stops all delivery.

M5 can come before M4, since M4 is optional polish. It must come after M2, because answering only matters once pelhaim runs trains it can answer.

Why this order:

- M1 delivers the owner's current orchestrator job, minus answering, with zero risk to running sessions. It runs alongside the Claude Code orchestrator (**[Owner], 2026-11-23**).
- M2 adds control over the CLIs the owner already trusts.
- M3, the biggest build, comes after, once the board and engine are proven.
- M5 adds a second client to an API that has been exercised since M1. That's why the seam ships early and the network parts late.

## 14. Open questions for the builder

Each has a default so work isn't blocked.

1. **Windows support?** The owner's setup repo targets Windows too (`bootstrap.ps1`). Default: not in v1. Keep the code portable (no macOS-only APIs outside the take-over terminal template).
2. **Exact Claude `system` subtypes that mean API failure.** Default: treat any subtype containing `error`, plus unknown records directly after an open call, as a soft-fail signal. Verify on real logs.
3. **Permission prompts in Claude JSONL.** Default: the §8.1 heuristic. Verify by triggering a prompt and reading the log.
4. **Codex subagents in rollout files.** Is `inter_agent_communication_metadata` the link? Default: no Codex subagent trains until verified.
5. **Codex questions and approvals in rollout files.** Default: none surfaced. Show turn-end results only.
6. **Claude `-p` host protocol** for permission and question requests over stream-json stdin/stdout, and whether `AskUserQuestion` exists in `-p`. Settle in the M2 spike. Fallback is in §8.2.1.
7. **`codex app-server`** (experimental) as a two-way launch transport instead of `exec`. Default: use `exec` + `exec resume`. Re-evaluate in the M2 spike.
8. **Take-over terminal mechanism on macOS** (Terminal vs iTerm2, a `.command` script vs AppleScript). Default: an `open -a` plus a generated script; the owner's terminal is set in config.
9. **Tool calling on local models.** Which local models handle native `tools` reliably? Default: native when the server reports it, else the fenced-JSON fallback.
10. **Non-Latin text** in transcripts and labels. Default: a fallback system font through `cosmic-text` / `swash`, for non-Latin only.
11. **Station inference in observe mode.** Settled by the owner on 2026-11-24: explicit `pelhaim-milestone:` markers only (§5.3, §7.10).
12. **Line ordering:** fixed config order vs float-on-alert. Default: config order, with lines holding open alerts floated up. DISPATCH is always first and Unsorted always last.
13. **`step_complete` transport for Claude Launch:** an MCP tool vs the final-line JSON. Default: the MCP tool. Settle in the M2 spike.
14. **Signing off a step that its dependants already branched from** (for example after reject-with-note produces new commits). Default: mark the dependants "base moved" and offer a rebase; never rebase automatically.
15. **Relay hosting** (owner decision, §8.7.4). Default if no answer: Railway, as the smallest always-on service. Nothing is deployed without the owner.
16. **Use an existing push service** (ntfy, Pushover) or Tailscale instead of our own relay? Default: our own relay, since none of the alternatives gives both answering and E2E.
17. **Native app stack** (owner decision). Default: SwiftUI + Notification Service Extension, iOS 17+ (for CryptoKit HPKE), in a separate `pelhaim-ios` repo.
18. **Can the phone approve high-risk alerts** (push/PR, security sign-off)? Default: no, view-only from the phone (`allow_high_risk = false`). Face ID to open the app defaults on.
19. **Notification option buttons:** dynamic notification categories vs answering in-app. Default: a text-reply action plus "Open" on the notification, and the option buttons in the app. Revisit once the M5 prototype runs.
20. **Claude `--permission-mode auto` semantics in `-p`** (§10.1). Confirm in the M2 spike. Fallback: `acceptEdits` plus `--allowedTools`.
21. **Codex `exec` network and approval behaviour under `-s workspace-write --approve-for-me`**, and the `-c` keys for approval policy. Confirm in the M2 spike, and don't guess the keys.

## 15. Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | Claude Code / Codex log formats change (undocumented) | High / high for observe | Tolerant parsers that skip the unknown; `TESTED_VERSIONS` and the "format untested" tick; `doctor`; fixtures per tested version; the adapter is the only place format knowledge lives |
| R2 | CLI flags or stream protocols drift (`-p` stream-json, `exec --json`) | Medium / high for launch | Verify with `--help` at build and in `doctor`; the M2 spike records real streams; fake-CLI tests pin the expected shape |
| R3 | The custom renderer eats time: text input, IME, a11y | Medium / medium | Bounded scope (one input widget); egui-for-console fallback (§9.2); scene snapshots keep layout work testable |
| R4 | Direct-mode tool execution without an OS sandbox | Medium / high | Now higher, since auto mode allows in-worktree exec without asking (owner, 2026-11-23). Mitigations: the §10.1 classifier sends outside-worktree, network, secrets and destructive work to the owner; worktree isolation; argv-not-shell; env scrub; no network tool; OS sandbox on the roadmap |
| R5 | Legibility vs the look: matte colours and a pixel font hurt readability | Medium / medium | `high_contrast` palette; tune the palette on the owner's real display; a11y names; state marks |
| R6 | Two writers on `DECISION-LOG.md` (pelhaim plus the owner's Claude orchestrator) | Medium / medium | Write only on click; re-read and check hash before an atomic rename; backup; refuse and alert on conflict. A single writer becomes possible once pelhaim fully replaces the orchestrator (owner, 2026-11-23). |
| R7 | Scope: three modes is a lot | High / medium | Strict milestone gates; M1 alone must be worth using daily |
| R8 | Privacy: transcripts contain secrets and private data | Medium / high | No transcript copies in observe; no bodies in logs; synthetic fixtures only; no telemetry |
| R9 | Cost runaway in Launch and Direct (parallel trains) | Medium / medium | `max_concurrent`; `--max-budget-usd` for Claude; per-train budgets in Direct; counters on tags |
| R10 | Film IP | Low / medium | No assets, frames, names or logos; original font; muted, re-derived palette; reference images stay outside the repo |
| R11 | Heuristic states mislead (a "probably waiting" that isn't) | Medium / low | Label heuristics as such in the alert text; heuristics are configurable and off-switchable |
| R12 | GPU goldens flaky in CI | Medium / low | Tolerance; lavapipe; fall back to scene snapshots in CI |
| R13 | The orchestrator forgets the tag, so work lands on Unsorted | Medium / low | Visible Unsorted line; one-click assign remembered per session; the tag rule goes into the owner's global agent rules |
| R14 | A Launch-mode CLI commits to `main` or pushes on its own, outside pelhaim's mediation | Low / high | Each Launch train's cwd is its worktree on a train branch; CLI permission settings; the owner's global rules; a post-step check that `main` and remotes haven't moved, which raises a red alert |
| R15 | Stacked branches drift when an upstream station changes after dependants branched | Medium / medium | "Base moved" mark; rebase offered, never automatic; PRs target the predecessor branch |
| R16 | Weak or missing acceptance checks make "done" mean little | Medium / medium | Checks are run by pelhaim, not the agent; project default check; `Manual` checks force sign-off; plan review shows checks per step |
| R17 | Auto mode lets an agent do damage inside, or leak out of, its worktree without asking (owner accepted auto mode, 2026-11-23) | Medium / high | Worktree per train, so damage is limited to a discardable branch; the §10.1 classifier sends outside-worktree, network, secrets and destructive work to the owner; Codex's workspace-write sandbox; post-step violation checks; per-project tightening |
| R18 | The CLIs' auto modes don't behave as assumed | Medium / medium | The M2 spike confirms them before launch mode ships; fallbacks in §10.1; `doctor` re-checks flags |
| R19 | Keep-awake drains the battery, or fails on battery or with the lid closed | Medium / low | `-s` only applies on AC; the indicator shows the battery caveat; click to pause; release after 60 s idle |
| R20 | The remote channel becomes an attack path into agent control | Low / high | Outbound-only connections; pairing with a code check; HPKE Auth-mode E2E; signed relay auth; replay protection; the narrow `RemoteApi`; high-risk alerts view-only by default; instant revocation |
| R21 | Remote costs and operations (the $99/yr Apple account, relay hosting, APNs key handling, app review) | High / low | Flagged as owner decisions; M5 is last and optional; the relay is small and stateless apart from a 24 h queue |
| R22 | Metadata leakage through the relay and APNs (timing, sizes, device tokens) | Medium / low | Documented in the README; payloads are ciphertext only; no alert text in APNs payloads |

## 16. Agent's calls the owner should check

**Status: approved by the owner on 2026-11-23**, together with the rest of the design, with the three changes in §5.2. Items 1–19 below are therefore owner-approved agent's calls. They stay listed so a later reader knows which parts started as agent defaults. Items 20 onward were added with §5.2 and are **awaiting owner review**.

These are listed in order of how much they shape the build.

1. The renderer: winit + wgpu + in-house, not macroquad or egui (§9.2). Approved 2026-11-23.
2. Engine/adapter split, event-sourced SQLite store (not Postgres), no transcript copies (§6). Approved 2026-11-23.
3. Decision log: the SQLite source of truth plus the two-writer export **is now owner-decided** (2026-11-23). Approved 2026-11-23 along with: the on-click mechanics, the hash-checked atomic writer, and automatic decision rows in pelhaim-as-orchestrator (§7.6, §8.5).
4. Train numbers = departure HHMM (§7.3); one block per tool call, never passing an unfinished station (§7.4). Approved 2026-11-23.
5. Cleared-route green is a darker green than the working train (§7.2, §9.1). Approved 2026-11-23.
6. The matte palette transform (chroma ×0.6, lightness ×0.88) and its start values (§9.1). Approved 2026-11-23.
7. Density cuts: ≤4 visible tracks per line, crossovers only for real merges, idle lines fold (§9.1). Approved 2026-11-23.
8. Observe stations = one per owner prompt; heuristics for "probably waiting" and "stalled" (§8.1). Approved 2026-11-23. Stations replaced by milestone markers, owner decision 2026-11-24 (§5.3).
9. Take over = an explicit detach (magenta), with the adapter swapped to observe (§8.2.4). Approved 2026-11-23.
10. One popup at a time, plus a tray; ✕ dismisses to the tray, never deletes (§9.4). Approved 2026-11-23.
11. Plan editing via `$EDITOR` TOML first, on-board editing later (§9.6). Approved 2026-11-23.
12. Security defaults: approval policy, no OS sandbox in v1, no dangerous flags unless set per project (§10). Worktree per train is now owner-decided (2026-11-23). Approved 2026-11-23.
13. Config at `~/.config/pelhaim/config.toml`; data at `~/.local/share/pelhaim/` (§6.5–6.6). Approved 2026-11-23.
14. Milestones M1–M4 and their acceptance criteria (§13). Approved 2026-11-23.
15. Tag format `pelhaim-project: <slug>` on the first prompt line, and the resolution order; subagents don't inherit a parent's tag (§7.7). Approved 2026-11-23.
16. Git mechanics: branch naming, merging a second predecessor into the train branch, PRs targeting the predecessor branch, "base moved" without auto-rebase, the post-step "main moved" check (§7.8). Approved 2026-11-23.
17. Checks are run by pelhaim, not the agent; `max_check_attempts = 3`; `Manual` checks force sign-off (§7.9). Approved 2026-11-23.
18. `step_complete` as an MCP tool, with final-line JSON as the fallback (§7.9). Approved 2026-11-23.
19. Routing-rule enforcement: a required `signoff` field per step, a keyword backstop, and a per-project "park and continue" flag, default off (§8.5). Approved 2026-11-23.
20. Auto-mode risk classes and defaults, the classifier lists, and loosening some classes drawing a red badge (§10.1). Awaiting owner review.
21. The CLI mapping: Claude `--permission-mode auto` plus `--disallowedTools` for push and merge; Codex `-s workspace-write --approve-for-me`. Both pending spike confirmation (§10.1). Awaiting owner review.
22. Keep-awake: `caffeinate -i -m -s -w <pid>` as a child process, not IOKit; no `-d`/`-u` by default; observed trains count; a 60 s release delay; waiting trains hold only when remote is on (§8.6). Awaiting owner review.
23. Remote design: outbound-only relay, HPKE Auth mode, QR pairing with code confirmation, replay rules, high-risk alerts view-only from the phone, M5 placement (§8.7, §13). Awaiting owner review.
24. Milestone marker details: own line, any line, not in fences, 32-char names, the `~h m s` estimate grammar up to 7 days, duplicates a no-op (§7.10). Awaiting owner review.
25. ETA factor: median of actual/estimate, newest 20, at least 5 samples, project then global, clamped 0.25–8; only the Active station shows `ARR`; the DISPATCH count includes grandchildren (§7.7, §7.10). Awaiting owner review.
