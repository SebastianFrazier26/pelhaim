# pelhaim: design

Date: 2026-11-23. Status: draft for owner review. No product code exists yet.

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
- Multi-user use, remote use, or a web UI. pelhaim is local and single-owner.
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
    fn risk(&self, input: &serde_json::Value) -> Risk;     // ReadOnly | WritesWorkspace | Executes | Network
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
name = "SafeAscent"                   # the line label
path = "~/Developer/safeascent"       # sessions whose cwd is under this path belong here
repo = "SebastianFrazier26/safeascent" # optional, for links
order = 1

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

[direct.approval]                     # per risk class: ask | allow | deny
read_only = "allow"
writes_workspace = "ask"
executes = "ask"
network = "deny"

[decision_log]
path = "~/Developer/DECISION-LOG.md"
write = "on_click"                    # on_click | off   (never "auto" in v1)

[board]
palette = "matte"                     # matte | film | high_contrast
motion = "full"                       # full | reduced
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
    id: ProjectId, name: String, root: PathBuf, repo: Option<String>, order: i32,
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
    status: StationStatus,        // Pending | Ready | Active | Done | Failed | Skipped
    train: Option<TrainId>,       // the train berthed or assigned here
    source: StationSource,        // Plan(step_id) | Inferred(observe heuristic)
}

struct Train {               // = Agent
    id: TrainId,
    number: u16,             // display id; see 7.3
    track: TrackId,
    position: Position,      // { leg: StationId → StationId, block: u16 }  — progress along the leg
    state: TrainState,       // Idle | Working | Waiting(AlertId) | Failed(reason) | Detached | Done | Stalled
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
    created: Timestamp, state: AlertState,   // Open | Tray (dismissed) | Answered{answer, by} | Expired
    answerable: bool,        // false for observed sessions
    decision: Option<DecisionId>,
}

struct Plan {
    id: PlanId, project: ProjectId, goal: String,
    steps: Vec<PlanStep>,    // { id, title, detail, depends_on, agent_hint: {tool|provider, model?}, acceptance }
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
- **Readiness:** a step is Ready when all its dependencies are Done.
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
- **Open decisions:** pelhaim reads each project's `### Open decisions (need you)` list too, and shows the count as a small yellow badge on the line label. This is read-only.

## 8. Event flow per mode

### 8.1 Observe mode

The adapter is `observe::claude` or `observe::codex`. Its capabilities are `read_only = true` and `can_answer = false`. **[Owner]** for read-only; the rest is **[Agent's call]**.

**Discovery and tailing**

- Watch the homes with the `notify` crate (FSEvents on macOS), with a 2 s poll as a fallback.
- Keep a byte offset per file. Read only up to the **last newline**: a final line without its newline is a record still being written. This is a loosy-goose lesson (`model.py::snapshot`).
- Parse each line as JSON. Skip invalid or non-object lines with a warning, never a crash.

**Claude Code** (`<claude_home>/projects/<cwd-slug>/<sessionId>.jsonl`)

- **Project:** each record carries `cwd`. Match it against `[[project]].path` by resolved-path prefix.
  - Don't trust the slug. loosy-goose notes the slug rule is inferred; the recorded `cwd` decides.
  - An unmatched cwd goes to an auto line named after the git root's directory, or "Unsorted".
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
- **Stations in observe mode** (no plan exists):
  - Each owner prompt (a `user` record with a real prompt, not a tool result or meta) starts a new station on the session's track, titled with the prompt's first ~40 characters.
  - Each subagent's single station is its `description`.
  - **[Agent's call]**. Alternative: one station per hour. Rejected because it carries no meaning.

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
  claude -p --output-format stream-json --input-format stream-json --verbose \
         --session-id <uuid> -n "pelhaim <train#> <station>" \
         [--model …] [--permission-mode …] [--add-dir …]
  ```
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
  - Fallback if no host protocol is usable: `--permission-mode` plus `--allowedTools` set by the owner's policy, and questions answered by sending the answer as the next user message on stdin.
- **Events:** each stream-json line is parsed into the same internal `Event`s as observe mode: tool use → step, result → berth, and so on.
  - The adapter *also* tails the session's own JSONL as a cross-check, which makes takeover seamless (§8.2.4).

#### 8.2.2 Codex

- **Spawn:**
  ```
  codex exec --json -C <project path> [-m <model>] [-s workspace-write] [--output-schema plan.schema.json] "<prompt>"
  ```
  All of these flags were verified.
- **Local models through Codex:** `--oss` and `--local-provider lmstudio|ollama` are verified. Launch mode can therefore also drive local models through Codex, independently of Direct mode.
- **Process life:** `codex exec` is one-shot; the process exits after the task.
  - A follow-up or answer is a new process: `codex exec resume <SESSION_ID> "<answer>"` (verified: `resume [SESSION_ID] [PROMPT]`). The train stays the same; only the process changes.
- **Approvals:** in exec mode, approval requests can't be answered interactively through `--json` stdout (inferred: the stream is one-way).
  - Options are a sandbox and approval policy set at launch from the owner's policy, or `--approve-for-me` (verified; it routes approvals to automatic review in workspace-write).
  - `codex app-server` (verified to exist, marked "[experimental]") may offer a two-way protocol with approval requests. Evaluating it is **[Open]** Q7.

#### 8.2.3 Flow

```
owner approves plan ─► engine: station Ready ─► Command::Launch{station, tool} ─► adapter spawns process
   stdout line ─► parse ─► Event::{TrainStep|AlertRaised|TrainBerthed|TrainFailed|SubagentSpawned}
   owner clicks an option in an alert ─► Command::Answer{alert, answer} ─► adapter writes to stdin
                                                                            (claude) / runs `exec resume` (codex)
   process exits 0 at task end ─► station Done ─► engine readies dependants ─► next launches
   process exits ≠0 / error event ─► train Failed (red), station Failed, dependants stay Pending
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
   └─ stop_reason end_turn ─► station Done ─► engine readies dependants
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
  - `write_file`, `edit_file`: risk WritesWorkspace.
  - `run`: risk Executes; an argv, not a shell string, by default.
  - `ask_user`, `spawn_agent`, `report`.
  - No network tool in v1.
- **The orchestration is the core, and is written code:** the plan DAG, scheduling, the approval gate, budgets, and subagent lifecycle and result routing. The model only fills in plans and does the work inside a station.

### 8.4 Mixed modes

Modes can run at once, e.g. observe all Claude/Codex sessions while launching new ones.

- A launched session is also visible to the observe adapter, because it writes JSONL.
- The engine dedups by session id: launch owns the train, and observe ignores session ids registered by launch.

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
│ SAFEASCENT ●2      plan ─── migrate 0007 ─── backfill ─── Stage C1 ─── sign-off           │  ← line label (+ open-decision badge), stations
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
  - *Approval gate:* there is a policy per risk class (config). Defaults:
    - read-only: allow;
    - writes: ask;
    - execute: ask;
    - network: deny.
    "Always allow for this train" is session-scoped and never persisted silently.
  - *Isolation:* by default each Direct train runs in its **own git worktree** on its own branch, so every change is reviewable and can be discarded.
  - *Process limits:* timeout, output cap, scrubbed environment. The child does not inherit `ANTHROPIC_API_KEY` or other secrets; there is an explicit allowlist.
  - *OS sandbox:* not in v1. macOS `sandbox-exec` is deprecated, and Linux options (Landlock, bubblewrap) need care. Called out as Risk R4.
- **Launch mode:**
  - pelhaim passes the owner's permission policy through to the CLIs.
  - It never adds `--dangerously-skip-permissions` or `--dangerously-bypass-approvals-and-sandbox` on its own. Those flags are exposed only if the owner sets them per project in config, with a red badge on the line label.
- **Untrusted content:**
  - Transcript text, tool output and model output are data. They are rendered as text and never interpreted as commands by pelhaim.
  - Links in transcripts open only on an explicit click.
- **Local providers:** a non-loopback `http://` base URL is refused unless `allow_insecure = true` is set for that provider.
- **No telemetry. Logs** (`tracing`, to `<data_dir>/logs`) carry ids, counts and states, never message bodies.
- **CI:**
  - `cargo audit`, or `cargo-deny` with advisories, licences and bans.
  - Dependencies pinned via the committed `Cargo.lock`.
  - Run `/secscan` before the first push.

## 11. Testing strategy

**[Agent's call]**

| Layer | How | What |
|---|---|---|
| `pelhaim-core` | Plain unit tests and `proptest` | Reducer: fixture event sequences lead to expected state. Plan validation: cycles, missing deps, duplicates. Path cover and layering. Train movement rules: never passes an unfinished station. Train numbering collisions. |
| Adapters, observe | Synthetic fixture homes under `tests/fixtures/{claude,codex}/…` written by hand to match today's shapes, **never real logs** | Parsing every record type listed in §8.1; partial last line; invalid JSON lines; subagent meta linking; `AskUserQuestion` open/close; stall and permission heuristics on a fake clock; the read-only assertion. |
| Adapters, launch | Fake `claude` and `codex` executables (small Rust test binaries) placed first on `PATH`, replaying recorded stream lines, reading stdin, and exiting with set codes | Spawn args are exactly what §8.2 says; answer routing; exit-code handling; take-over handoff. Fixture streams come from the M2 spike recordings, scrubbed. |
| Providers | `wiremock` HTTP server | Anthropic SSE and OpenAI-compatible streaming, tool-call assembly from deltas, errors, retries, key never in logs (assert on captured logs). |
| Runtime | A scripted fake `Provider` | Approval gate per risk class; path confinement (escape attempts); budgets; `spawn_agent` / `ask_user` round trips; worktree creation. |
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
- Console strip with local commands only.
- AccessKit basics.
- **Done when:**
  - with the owner's real 7 projects configured, every active Claude Code and Codex session shows as a train on the right line;
  - an `AskUserQuestion` in any session raises a yellow train and an alert within 2 s;
  - answering in the terminal closes it;
  - a decision can be logged into `DECISION-LOG.md` without disturbing the rest of the file;
  - CI is green on both OSes.

**M2: Launch mode + planning**

1. A **spike**: record real `claude -p` stream-json and `codex exec --json` sessions, including a permission prompt, a question and a subagent. Settle [Open] Q6 and Q7, and commit scrubbed fixtures.
2. `launch::claude`, `launch::codex`; answering; take over and detach; dedup with observe.
3. Dispatcher planning via a launched session with `--json-schema` / `--output-schema` to produce Plan JSON. Plan review alert, `$EDITOR` editing, approval, the scheduler, and cleared-route rendering.
4. The console sends to the dispatcher; `@train` messages.
- **Done when:** the owner can type a goal, approve a 4-step plan with one dependency fork, and watch 2 trains run in parallel, answer one question from the board, take over one train in a terminal, and see all of it land on the board.

**M3: Direct mode**

- `Provider` trait, `anthropic`, `openai_compat` (tested against a local Ollama or LM Studio).
- Agent loop, tools, approval gate, worktree isolation, budgets, `spawn_agent` / `ask_user`.
- The planner runs on a provider.
- **Done when:** the same M2 acceptance run works with Direct mode on Anthropic, and on one local model, with every write and exec going through the approval gate.

**M4: Polish** (optional)

- On-board plan editing (drag dependencies), hand back after take-over, wall mode across multiple windows, the on-set post pass, Windows, a bundled fallback font.

Why this order:

- M1 delivers the owner's current orchestrator job, minus answering, with zero risk to running sessions.
- M2 adds control over the CLIs the owner already trusts.
- M3, the biggest build, comes last, once the board and engine are proven.

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
11. **Station inference in observe mode.** Default: one station per owner prompt (§8.1). The builder may propose better once real data is on the board.
12. **Line ordering:** fixed config order vs float-on-alert. Default: config order, with lines holding open alerts floated up.

## 15. Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | Claude Code / Codex log formats change (undocumented) | High / high for observe | Tolerant parsers that skip the unknown; `TESTED_VERSIONS` and the "format untested" tick; `doctor`; fixtures per tested version; the adapter is the only place format knowledge lives |
| R2 | CLI flags or stream protocols drift (`-p` stream-json, `exec --json`) | Medium / high for launch | Verify with `--help` at build and in `doctor`; the M2 spike records real streams; fake-CLI tests pin the expected shape |
| R3 | The custom renderer eats time: text input, IME, a11y | Medium / medium | Bounded scope (one input widget); egui-for-console fallback (§9.2); scene snapshots keep layout work testable |
| R4 | Direct-mode tool execution without an OS sandbox | Medium / high | Approval gate defaults to ask; worktree isolation; argv-not-shell; env scrub; no network tool; OS sandbox on the roadmap |
| R5 | Legibility vs the look: matte colours and a pixel font hurt readability | Medium / medium | `high_contrast` palette; tune the palette on the owner's real display; a11y names; state marks |
| R6 | Two writers on `DECISION-LOG.md` (pelhaim plus the owner's Claude orchestrator) | Medium / medium | Write only on click; re-read and check hash before an atomic rename; backup; refuse and alert on conflict. Longer-term, the owner picks one writer. |
| R7 | Scope: three modes is a lot | High / medium | Strict milestone gates; M1 alone must be worth using daily |
| R8 | Privacy: transcripts contain secrets and private data | Medium / high | No transcript copies in observe; no bodies in logs; synthetic fixtures only; no telemetry |
| R9 | Cost runaway in Launch and Direct (parallel trains) | Medium / medium | `max_concurrent`; `--max-budget-usd` for Claude; per-train budgets in Direct; counters on tags |
| R10 | Film IP | Low / medium | No assets, frames, names or logos; original font; muted, re-derived palette; reference images stay outside the repo |
| R11 | Heuristic states mislead (a "probably waiting" that isn't) | Medium / low | Label heuristics as such in the alert text; heuristics are configurable and off-switchable |
| R12 | GPU goldens flaky in CI | Medium / low | Tolerance; lavapipe; fall back to scene snapshots in CI |

## 16. Agent's calls the owner should check

These are listed in order of how much they shape the build.

1. The renderer: winit + wgpu + in-house, not macroquad or egui (§9.2).
2. Engine/adapter split, event-sourced SQLite store (not Postgres), no transcript copies (§6).
3. Decision log: SQLite is the source of truth; markdown is written only on click, with a safe atomic writer (§7.6).
4. Train numbers = departure HHMM (§7.3); one block per tool call, never passing an unfinished station (§7.4).
5. Cleared-route green is a darker green than the working train (§7.2, §9.1).
6. The matte palette transform (chroma ×0.6, lightness ×0.88) and its start values (§9.1).
7. Density cuts: ≤4 visible tracks per line, crossovers only for real merges, idle lines fold (§9.1).
8. Observe stations = one per owner prompt; heuristics for "probably waiting" and "stalled" (§8.1).
9. Take over = an explicit detach (magenta), with the adapter swapped to observe (§8.2.4).
10. One popup at a time, plus a tray; ✕ dismisses to the tray, never deletes (§9.4).
11. Plan editing via `$EDITOR` TOML first, on-board editing later (§9.6).
12. Security defaults: approval policy, worktree per Direct train, no OS sandbox in v1, no dangerous flags unless set per project (§10).
13. Config at `~/.config/pelhaim/config.toml`; data at `~/.local/share/pelhaim/` (§6.5–6.6).
14. Milestones M1–M4 and their acceptance criteria (§13).
