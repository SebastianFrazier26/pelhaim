# M1 acceptance run (owner checklist)

Spec §13 "Done when", run by the owner against the real homes. Observe is read-only, but step 5 writes the real `~/Developer/DECISION-LOG.md`: do it only if you want that entry. Write results in the hand-off report, not in this repo.

## 0. Before

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd ~/Developer/pelhaim-m1
cargo build --release -p pelhaim
touch /tmp/pelhaim-marker
cp ~/Developer/DECISION-LOG.md ~/Developer/DECISION-LOG.md.pre-acceptance
```

Your config (`~/.config/pelhaim/config.toml`) lists the real projects, one `[[project]]` each.

## 1. Doctor

```sh
./target/release/pelhaim doctor
```

Look for:

- `claude: <version> (tested 2.1.331)` and `codex: <version> (tested 0.162.1)`.
- `config: ~/.config/pelhaim/config.toml ok`.
- `data dir: … writable`, `decision log: … found`.
- `keep awake: caffeinate`, `gpu: Apple … (Metal)`.
- `format untested:` lists only sessions written by a CLI newer than tested (they carry a ⚠ tick on the board).
- No key or token value anywhere in the output.

## 2. Observe

```sh
./target/release/pelhaim observe
```

Check, ticking each:

1. [ ] Every tagged Claude Code and Codex session or agent is on its project's line.
1a. [ ] (Owner decision 2026-11-24.) With no `[[project]]` in config, an untagged session running in a repository or one of its worktrees (e.g. `~/Developer/SafeAscent-r7v2`) is on a line named after the main repository (`SafeAscent`), with no Assign alert. A subagent of the orchestrator whose cwd is a repository does the same, and never lands on DISPATCH. Only sessions with no tag and no repository (e.g. cwd `~/Developer`) are on UNSORTED, each with an Assign alert. With `[board].auto_projects = false`, untagged sessions are back on UNSORTED.
2. [ ] (Owner decision 2026-11-24.) The orchestrator draws no train once it is tagged `dispatch`, listed in `[dispatch].sessions`, or marked "This is the orchestrator" on its Assign popup. DISPATCH is one row, `DISPATCH · N agents running`, and N matches the orchestrator's working subagents. Its tagged subagents run on their own lines with no fork back to DISPATCH; untagged ones land on UNSORTED with an Assign alert (DISPATCH is never inherited). A question from the orchestrator still opens a popup and turns the count yellow.
2a. [ ] (Owner decision 2026-11-24.) Prompts make no stations: a line whose sessions never print a marker has no stations, and its train stops short of the line's end however many tool calls it makes.
2b. [ ] (Owner decision 2026-11-24.) Have an agent print, on lines of their own, `pelhaim-milestone: plan ~2m` and later `pelhaim-milestone: build ~1h`. A station appears for each; `plan` turns done grey when `build` starts; `build` shows `ARR HH:MM` (local time, about an hour after the marker). A marker inside a code fence or mid-sentence makes nothing; repeating `plan` makes nothing.
2c. [ ] (Owner decision 2026-11-24.) Wait past a short estimate (`pelhaim-milestone: quick ~1m`): after the clock passes, the `ARR` text and the station square turn amber.
2d. [ ] (Owner decision 2026-11-24.) Six markers on one line show five stations and `+1 earlier` (or your `[board].max_stations`).
2f. [ ] (Owner decision 2026-11-24, board lines.) Each project has exactly one train, however many sessions and subagents run on it; its tag shows the most urgent state (a waiting session turns it yellow even while others work) and double-clicking it lists the agents inside. Milestones from two agents on the same project appear as stations on the one line, in the order they were printed. No forks or sub-tracks are drawn.
2g. [ ] (Owner decision 2026-11-24, board lines.) Lines have different base colours and 45° turns, keep the same shape and colour across restarts, and run about 88% of the window width even with no stations. Setting `color = "orange"` (or `"#RRGGBB"`) on a `[[project]]` changes that line's colour; an invalid value is a config error naming `project[N].color`. No line runs through a label. With `palette = "high_contrast"` lines stay distinct and readable on black.
2e. [ ] (Owner decision 2026-11-24.) After a few stations with estimates finish, `sqlite3 ~/.local/share/pelhaim/pelhaim.db 'select * from station_timings'` shows only keys, project slugs and numbers, never a station name.
3. [ ] Ask an `AskUserQuestion` in any session: the train turns yellow and a popup opens within 2 s.
4. [ ] Answer it in the session's terminal: within 2 s the popup flips to `STATUS: ANSWERED` and stays open to offer `[Log decision]`, until you close or dismiss it. A logged title carries only your answer, never the harness's "Your questions have been answered…" text.
5. [ ] (Optional, writes the real log.) Click `[Log decision]` on an answered alert, then:

   ```sh
   git diff --no-index ~/Developer/DECISION-LOG.md.pelhaim-bak ~/Developer/DECISION-LOG.md
   ```

   The diff shows only the inserted entry.
6. [ ] While an agent works, the AWAKE lamp is lit, and `pmset -g assertions` lists `caffeinate` (`/usr/bin/caffeinate -i -m -s -w <pid>`). It goes out about 60 s after the last agent stops.
7. [ ] After quitting:

   ```sh
   find ~/.claude ~/.codex -newer /tmp/pelhaim-marker -path '*pelhaim*'
   ```

   prints nothing, and nothing under `~/.claude` or `~/.codex` gained a pelhaim write.

## 3. Visual and input checks

8. [ ] Tag and station colours match the matte legend in the README: working green, waiting yellow, failed red, done grey, idle pale teal, indigo background `#0D0B21`, labels `#B4B4B7`. Line colours are darker than every tag.
9. [ ] Glyphs are crisp at your display's scale: every pixel of the pixel font has the same width, with no blur or uneven columns. Try ⌘= / ⌘- and `0`.
10. [ ] Double-click a **yellow** tag: the zoom inset opens on that train (an alert popup over the tag must not swallow the second click). A single click on it opens its alert.
11. [ ] Press `/` and type in the console with an IME (for example Japanese kana → kanji, or a dead key such as ⌥e e → é). The preedit shows underlined while composing, the committed text lands once (no doubled characters, no stray `/`), and `Esc` leaves the console.
12. [ ] `Tab` cycles alerts, `J`/`K` step trains, `Space` pauses motion, `Esc` closes the popup, then the inset.

## 4. After

```sh
diff ~/Developer/DECISION-LOG.md.pre-acceptance ~/Developer/DECISION-LOG.md   # empty unless you did step 5
rm /tmp/pelhaim-marker
```

CI green on macOS and Linux is the last §13 item. It needs a push (`/commitandpush`).
