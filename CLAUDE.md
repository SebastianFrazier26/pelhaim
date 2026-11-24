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
- Run: `cargo run -p pelhaim -- observe`
- Gate (must pass before every commit):

  ```sh
  cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  ```

- Licences and advisories: `cargo deny check`

## Layout

`crates/pelhaim-core` (pure engine, no IO) ← `pelhaim-store`, `pelhaim-adapters`, `pelhaim-board`, `pelhaim-remote`; `pelhaim-board` ← `pelhaim-gpu`; everything ← `crates/pelhaim` (the binary, with logic in `src/lib.rs` and a thin `main.rs`).

## Rules

- Never copy anything from `~/Developer/pelhaim-reference` into this repo.
- Observe code never writes: adapters open session files with `File::open` only, and never write, rename, create or delete anything under `~/.claude` or `~/.codex`, or touch another process.
- No transcript text is persisted or logged.
- No `unsafe` (workspace lint `unsafe_code = "deny"`).
- Add dependencies with `cargo add`; commit `Cargo.lock`.

## Comments and CHANGELOG

- Comments are load-bearing only: a why, a constraint, or a deliberately odd construct. No narration.
- Every user-visible change adds `- YYYY-MM-DD: …` under `## Unreleased` in `CHANGELOG.md`, in the same commit.
