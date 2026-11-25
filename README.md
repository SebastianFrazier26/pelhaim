# pelhaim

pelhaim is a desktop board for running and watching AI coding agents. Each project is a train line, each agent session is a train on it, each milestone the agent reports is a station, and when an agent stops to ask you something, an alert pops up on the board. It runs locally and works with Claude Code and Codex.

The look is inspired by the control-room board in a 2009 film.

Screenshot: to come.

## Modes

pelhaim has three modes. The board looks the same in all of them.

- **Observe** reads the session files Claude Code (`~/.claude/projects`) and Codex (`~/.codex/sessions`) already write, and draws every session on the board. It is strictly read-only: it never writes to those directories and never types into a session. You answer questions in the session's own terminal.
- **Launch** (planned) starts sessions itself so pelhaim can answer their questions.
- **Direct** (planned) makes pelhaim the agent runtime, calling model APIs directly.

This build ships **Observe only**.

## What the board shows

- **Lines** are projects from your config, in `order`. **DISPATCH** is always the top row: a one-line strip, `DISPATCH · 6 agents running`, counting the agents your orchestrator has running. The orchestrator itself draws no train, but its questions still pop up. **UNSORTED** is the bottom band and holds sessions pelhaim cannot place.
- **Trains** are sessions (Claude Code sessions and subagents, Codex rollouts). An orchestrator's subagents appear on their own project lines. A train steps one block per tool call, on a track of bounded length.
- **Stations** are milestones the agent reports with a marker line (see below). Prompts never make stations. At most `max_stations` show per track (default 5); older ones fold into `+N earlier`. The station a train is running to shows its arrival clock, `ARR 14:52`, which turns amber when overdue.

Colour legend (default `matte` palette):

| Colour | Meaning |
|---|---|
| green `#76A96F` | working |
| dark green `#466C41` | cleared route ahead of a working train |
| yellow `#CCC984` | waiting on you: a question, a permission prompt, a stall |
| red `#BF6661` | failed |
| purple `#A86AB8` | forked / detached |
| grey `#85889A` | done |
| pale teal `#99B5B7` | idle |

A red outline on a train's tag means a mis-tag (a slug not in your config). A small yellow badge on a line label counts that project's open decisions in `DECISION-LOG.md`; a ⚠ tick means a session was written by a CLI version newer than the tested ones (`pelhaim doctor` lists them).

## Tags: putting work on the right line

pelhaim never guesses a line from the working directory. The first line of a session's first prompt, or of a subagent's spawn prompt, must be exactly:

```
pelhaim-project: <slug>
```

`<slug>` is `[a-z0-9-]+` and must match a `[[project]].slug`. The reserved slug `dispatch` marks an orchestrator and puts it on DISPATCH (or list its session id under `[dispatch].sessions`). An untagged session goes to UNSORTED with an Assign alert listing your projects; your answer is stored and survives restarts. Tell your orchestrator to prefix every spawn prompt with the tag.

## Milestones: stations and arrival times

A station appears when a session's text (assistant output or your prompt) has a line that is exactly:

```
pelhaim-milestone: <name>
pelhaim-milestone: <name> ~<estimate>
```

The marker must be on its own line (leading spaces are fine) and outside code fences. Names are cut to 32 characters; a name already used on the same track is ignored. The estimate is optional: `~45s`, `~20m`, `~1h30m` (units `h`, `m`, `s`, in that order, up to 7 days). A marker closes the previous station and starts the next.

The arrival clock is the marker's time plus the estimate times a learned factor. pelhaim records each finished station's estimate and actual time (numbers only, never the name) and uses the median actual/estimate ratio of the last 20, per project once it has 5, else across all projects, else 1. Tell your agents to emit a marker, with an honest estimate, as they start each step.

## Config

`~/.config/pelhaim/config.toml` (override with `PELHAIM_CONFIG` or `--config`). Unknown keys are errors. A missing file means defaults.

```toml
mode = ["observe"]

[observe]
active_window = "48h"      # sessions touched within this window get a train
stall_after = "10m"        # "off" disables
permission_after = "20s"

[[project]]
slug = "safeascent"
name = "SafeAscent"
order = 1
log_section = "SafeAscent" # heading in DECISION-LOG.md, default: name

[dispatch]
sessions = []              # session ids drawn on DISPATCH

[keep_awake]
enabled = true
include_observed = true
keep_display_awake = false

[decision_log]
path = "~/Developer/DECISION-LOG.md"
write = "on_click"         # on_click | off

[board]
palette = "matte"          # matte | film | high_contrast
motion = "full"            # full | reduced
max_stations = 5           # stations shown per track (1–20)
```

Sections for later milestones (`[launch]`, `[direct]`, `[approval]`, `[project_overrides]`, `[git]`) are rejected with the milestone that brings them.

## Install and run

You need Rust; the exact toolchain is pinned in `rust-toolchain.toml` and `rustup` installs it on first build.

```sh
cargo run --release -p pelhaim -- observe     # open the board (the bare `pelhaim` does the same in M1)
cargo run --release -p pelhaim -- doctor      # CLI versions vs tested, config, data dir, log, keep-awake, GPU
cargo run --release -p pelhaim -- decisions   # logged decisions, newest first; --open lists open ones
cargo run --release -p pelhaim -- --headless --dump-scene out.json   # catch up once, write the scene, exit
```

Data lives in `~/.local/share/pelhaim/` (override `PELHAIM_DATA_DIR`): `pelhaim.db` and `logs/pelhaim.log`.

Run the tests with `cargo test --workspace`.

## Keyboard and mouse

| Input | Action |
|---|---|
| Click a tag | Select the train; its status shows in the console strip. |
| Double-click a tag, or `Enter` | Open the zoom inset on that train. |
| Click a yellow tag | Open its alert. |
| `Tab` / `Shift-Tab` | Cycle trains with open alerts, oldest first. |
| `J` / `K` | Next / previous train. |
| `1`–`9`, `Enter` | Highlight an alert option, then send it. |
| `Esc` | Close the topmost popup or inset, then clear the selection. |
| `Space` | Pause board motion (state still updates). |
| `0` / `F` | Fit the whole board / the selection. |
| `⌘=` / `⌘-`, pinch, scroll | Zoom and pan. |
| `⌘↑` | Toggle the radio log. |
| `/` | Focus the console: `/goto <train>`, `/decisions`, `/decisions export`, `/mode`, `/help`. |

## Guarantees

- **Read-only observe.** pelhaim only opens session files under `~/.claude` and `~/.codex` for reading. It never writes, renames, creates or deletes anything there, and never types into or signals a session. You answer questions in the session's own terminal.
- **Privacy.** No transcript text is copied to disk or logged: the database holds your assignments, dismissals and decisions, and the log holds ids, counts and states. There is no telemetry and no network code.
- **Decision log.** `DECISION-LOG.md` is written only when you click `[Log decision]`, by inserting one entry (atomic rename, previous version kept as `DECISION-LOG.md.pelhaim-bak`).

## Keep-awake

While any observed train is working, pelhaim holds the Mac awake with `/usr/bin/caffeinate -i -m -s -w <pid>` (Linux: `systemd-inhibit`, unverified), released 60 s after the last one stops. The AWAKE lamp in the console strip shows it; clicking it pauses the hold for an hour. Battery caveat: on battery only idle and disk sleep are prevented, so closing the lid still sleeps the machine.

## License

MIT. See `LICENSE`.
