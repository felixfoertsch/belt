# belt

`belt` is one CLI for an Uberspace asteroid belt. It keeps local asteroid inventory, detects U7 or U8 remotely, wraps public on-server `uberspace` commands over SSH, and presents cached fleet status in human-readable or JSON form.

> Dashboard account and asteroid lifecycle operations are not implemented yet. Uberspace exposes no public dashboard API; those commands wait for captured, redacted network traffic and verified request semantics.

## Features

- Manage named local asteroid inventory.
- Detect U7 (CentOS 7) and U8 (Arch Linux) from `/etc/os-release`.
- Translate supported canonical commands across U7 and U8.
- Run public, space-separated remote `uberspace` commands.
- Refresh fleet status with bounded SSH and command timeouts.
- Cache status atomically under `$XDG_CONFIG_HOME/belt`.
- Emit terminal output or JSON.
- Import legacy `asteroids.list` and `~/.config/uc/registry.toml` data.

## Install

Download matching binary from [GitHub Releases](https://github.com/felixfoertsch/belt/releases), make it executable, and place it in `PATH`:

```fish
chmod +x belt-aarch64-apple-darwin
mkdir -p ~/.local/bin
mv belt-aarch64-apple-darwin ~/.local/bin/belt
belt --help
```

Available release targets:

- `belt-aarch64-apple-darwin` — Apple Silicon macOS
- `belt-x86_64-unknown-linux-musl` — static x86-64 Linux

## Build from source

Requirements: [mise](https://mise.jdx.dev/) and Git.

```fish
git clone git@github.com:felixfoertsch/belt.git
cd belt
mise install
mise run check
mise run build -- macos
```

macOS artifact: `dist/belt-aarch64-apple-darwin`.

Linux cross-compilation additionally needs Zig, `cargo-zigbuild`, and Rust target `x86_64-unknown-linux-musl`:

```fish
mise run build -- linux
```

## Quick start

Register asteroids:

```fish
belt add danger cetus.uberspace.de 7
belt add impstr pandora.uberspace.de 8
belt list
```

Import legacy inventory:

```fish
belt import /path/to/asteroids.list
```

Registry lives at `$XDG_CONFIG_HOME/belt/registry.toml`, normally `~/.config/belt/registry.toml`. If no belt registry exists, `~/.config/uc/registry.toml` migrates automatically without deleting its source.

## Remote commands

Command shape:

```text
belt <asteroid> <uberspace command...>
```

Examples:

```fish
belt danger web domain list
belt danger mail user list
belt impstr port list
belt impstr records show example.org
belt impstr tools version list
```

Canonical forms mostly follow U7 syntax. For U8, belt translates verified differences such as:

- `mail user` → `mail address`
- `port list` → `web backend list`
- `records show` → `web domain show`
- `tools version` → `tool version`

Unknown forms pass through unchanged for generation-specific commands. Unsupported translations fail explicitly. Full matrix: [`docs/commands.md`](docs/commands.md).

## Status

```fish
belt danger status
belt status --refresh
belt status
belt status --json
belt status --refresh --json
```

Refresh records domains, mail configuration, and U7 ports or U8 backends. Failed or incomplete refreshes do not replace last known good cache.

## SSH setup

belt requires:

- `ssh` in `PATH`
- key-based access to `<account>@<server>`
- trusted host keys in `~/.ssh/known_hosts`
- public remote `uberspace` CLI

Automated probes enforce `BatchMode=yes`, `StrictHostKeyChecking=yes`, a 10-second connection timeout, bounded keepalives, and per-command timeout. Interactive passthrough requests a TTY only when local input and output are terminals.

Verify host fingerprints through a trusted Uberspace source before adding them. Never store passwords, private keys, session cookies, or database credentials in belt configuration.

## Development

```fish
mise run check
mise run build -- macos
```

GitHub Actions tests every push and pull request. Pushes to `main` publish a CalVer release (`YYYY.MM.DD`, then `YYYY.MM.DD.N` for another release on same day) with macOS ARM64 and static Linux x86-64 binaries.

## Uberspace manuals

- [U8 manual](https://u8manual.uberspace.de/)
- [U7 manual](https://manual.uberspace.de/)

## Project status

Remote management works. Dashboard management remains intentionally pending until its private HTTP behavior is captured and verified.
