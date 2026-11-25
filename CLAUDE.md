# pelhaim — agent notes

Rust workspace: a subway-style board that observes Claude Code and Codex sessions. Spec: `docs/superpowers/specs/`. Plans: `docs/superpowers/plans/`.

## PATH

`cargo` is not on PATH in non-interactive shells on this machine. Start every shell with:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Run: `cargo run -p pelhaim -- observe` (also `doctor`, `decisions [--open]`, `--headless --dump-scene out.json`)
- Gate (must pass before every commit):

  ```sh
  cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  ```

- Licences and advisories: `cargo deny check`
- App icon: `tools/icon/build.sh` (PNGs, iconset and `.icns` under `assets/icon/`); macOS app: `scripts/bundle-macos.sh [--install]` → `target/pelhaim.app`

## Layout

`crates/pelhaim-core` (pure engine, no IO) ← `pelhaim-store`, `pelhaim-adapters`, `pelhaim-board`, `pelhaim-remote`; `pelhaim-board` ← `pelhaim-gpu`; everything ← `crates/pelhaim` (the binary, with logic in `src/lib.rs` and a thin `main.rs`).

- `pelhaim-core`: the reducer (`Engine`), events, commands, snapshot, tag and milestone-marker parsing, the ETA correction factor, train numbers.
- `pelhaim-store`: SQLite owner events and projections; the `DECISION-LOG.md` writer and open-decision reader.
- `pelhaim-adapters`: read-only tailer, home watcher, `ObserveClaude` / `ObserveCodex`, tested CLI versions.
- `pelhaim-board`: palette, pixel font, layout, scene (display list + hits), input handling, a11y tree.
- `pelhaim-gpu`: wgpu renderer, winit app (`run_app`, `UiBackend`), offscreen render.
- `pelhaim-remote`: the `RemoteApi` seam and compact snapshot.
- `crates/pelhaim/src`: `cli` (clap), `config`, `env`, `run` (wiring, headless dump, `decisions`), `doctor`, `backend` (UI → engine), `engine_task`, `store_worker`, `keep_awake`.

## Fixtures and tests

- Fixtures are synthetic and hand-written, never copied from real session files: `crates/pelhaim-adapters/tests/fixtures/{claude,codex}` and `crates/pelhaim/tests/fixtures/config`.
- Tests never touch the real `~/.claude`, `~/.codex`, `~/.config/pelhaim` or `~/Developer/DECISION-LOG.md`: they copy fixtures into a tempdir and inject paths through an `Env` or a child's env (`HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PELHAIM_DATA_DIR`, `PELHAIM_CONFIG`, `PATH`).

## Rules

- Never copy anything from `~/Developer/pelhaim-reference` into this repo.
- Observe code never writes: adapters open session files with `File::open` only, and never write, rename, create or delete anything under `~/.claude` or `~/.codex`, or touch another process.
- No transcript text is persisted or logged.
- No `unsafe` (workspace lint `unsafe_code = "deny"`).
- Add dependencies with `cargo add`; commit `Cargo.lock`.

## Comments and CHANGELOG

- Comments are load-bearing only: a why, a constraint, or a deliberately odd construct. No narration.
- Every user-visible change adds `- YYYY-MM-DD: …` under `## Unreleased` in `CHANGELOG.md`, in the same commit.
