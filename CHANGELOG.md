# Changelog

## Unreleased

- 2026-11-23: Workspace scaffold and CI.
- 2026-11-23: DECISION-LOG.md export: inserts a decision at the top of its project's Decided list (hash-checked atomic write, backup, idempotent per decision) and reads each project's open decisions.
- 2026-11-23: Read-only session-file tailer (complete lines only, CRLF-tolerant, resets on truncate/replace, reads at most 4 MiB per call) and a recursive home watcher with a poll tick.
