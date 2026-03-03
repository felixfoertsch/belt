# AI Agent Report — uberspace-cli

## Status

Phase 1 + 2 of the Rust rewrite are complete. The `uc` binary compiles and all 24 unit tests pass.

## What was done

- Created Rust project (`Cargo.toml`, binary name `uc`, CalVer `2026.3.3`)
- Implemented all Phase 1 + 2 modules:
  - `registry.rs` — TOML-based asteroid CRUD (`~/.config/uc/registry.toml`)
  - `ssh.rs` — passthrough (`ssh -t`) and capture (for status collection)
  - `cache.rs` — per-asteroid TOML status cache (`~/.config/uc/cache/*.toml`)
  - `translate.rs` — v7/v8 command translation (mail, port, tools, web backend)
  - `status.rs` — remote status collection via SSH, colored display, age/staleness
  - `cli.rs` — clap-derived CLI with fallback to asteroid passthrough
- `uc import <path>` migrates legacy `asteroids.list` + cache files

## CLI command tree

```
uc add <name> <server> <version>     # register asteroid
uc list                               # list registered
uc remove <name>                      # deregister
uc status [--refresh]                 # aggregate from cache (or refresh all)
uc <name> status                      # refresh + show one asteroid
uc <name> <args...>                   # passthrough → SSH with v7/v8 translation
uc import <path>                      # migrate from asteroids.list
```

## Architecture notes

- Clap `try_parse()` + manual fallback handles the `uc <name> <args...>` passthrough pattern
- SSH passthrough preserves the user's SSH config/keys — no Rust SSH library needed
- Remote status script is the same bash heredoc from the original, embedded as a const
- v8 Rich-table box-drawing chrome is stripped by the remote script before parsing

## What's next (Phase 3)

- Dashboard HTTP API: `reqwest` + `serde_json` for login, create-asteroid, add-ssh-key, delete
- Session management: cookies + CSRF token in `~/.config/uc/session.toml`

## Known considerations

- The `cache::load()` function exists but is currently only used transitively via `load_all()` — produces a dead-code warning
- Edition 2024 requires Rust 1.85+; current toolchain is 1.93
