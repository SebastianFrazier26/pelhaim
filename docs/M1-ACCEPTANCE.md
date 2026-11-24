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

1. [ ] Every tagged Claude Code and Codex session or agent is on its project's line. Untagged ones are on UNSORTED, each with an Assign alert.
2. [ ] The orchestrator session is on DISPATCH after you answer "This is the orchestrator" on its Assign popup (or list its id in `[dispatch].sessions`). Its tagged subagents branch off onto their lines; untagged ones land on UNSORTED with an Assign alert (DISPATCH is never inherited).
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

8. [ ] Colours match the matte legend in the README: working green, waiting yellow, failed red, done grey, idle pale teal, indigo background `#0D0B21`, labels `#B4B4B7`.
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
