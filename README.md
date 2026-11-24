# pelhaim

pelhaim is a desktop board for running and watching AI coding agents. Each project is a train line, each agent session is a train on it, each task the agent works through is a station, and when an agent stops to ask you something, an alert pops up on the board. It runs locally and works with Claude Code and Codex.

The look is inspired by the control-room board in a 2009 film.

Screenshot: to come.

## Modes

pelhaim has three modes. The board looks the same in all of them.

- **Observe** reads the session files Claude Code (`~/.claude/projects`) and Codex (`~/.codex/sessions`) already write, and draws every session on the board. It is strictly read-only: it never writes to those directories and never types into a session. You answer questions in the session's own terminal.
- **Launch** (planned) starts sessions itself so pelhaim can answer their questions.
- **Direct** (planned) makes pelhaim the agent runtime, calling model APIs directly.

This build ships **Observe only**.

## Install and run

You need Rust; the exact toolchain is pinned in `rust-toolchain.toml` and `rustup` installs it on first build.

```sh
# from a clone of this repository
cargo run -p pelhaim -- observe
```

Run the tests with `cargo test --workspace`.

## License

MIT. See `LICENSE`.
