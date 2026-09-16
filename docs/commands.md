# `belt` command model

## Local inventory

| Command | Effect |
|---|---|
| `belt add <name> <host> <7|8>` | Add or replace local asteroid metadata |
| `belt list` | List local asteroids |
| `belt remove <name>` | Remove local asteroid and cache |
| `belt import <asteroids.list>` | Import validated legacy inventory |
| `belt status [--refresh]` | Show or refresh aggregate remote status |

## Remote wrapper

`belt <name> <command...>` detects remote generation from `/etc/os-release`, checks registered metadata, translates supported canonical commands, then runs public space-separated `uberspace` CLI.

| Task | U7 | U8 |
|---|---|---|
| Domains | `web domain list/add/del` | `web domain list/add/del` |
| Domain details | `records show <domain>` | `web domain show <domain>` |
| Backends | `web backend list/set` | `web backend list/add` |
| Mailboxes | `mail user list/add/del` | `mail address list/add/del` |
| Runtime versions | `tools version ...` | `tool version ...` |
| Ports | `port list` | represented by `web backend list` |
| Services | `supervisorctl` outside public CLI | `service ...` |

Current canonical translations:

- `mail user list` → U8 `mail address list`
- `mail user add/del` → U8 `mail address add/del`
- `port list` → U8 `web backend list`
- `tools version use` → U8 `tool version set`
- `web backend set <path> --http --port <n>` → U8 `web backend add <path> port <n> --force`

`tools restart` fails on U8 because no verified equivalent exists. Unknown forms pass through unchanged, allowing generation-specific commands. Remote CLI availability still varies by space; inspect `belt <name> --help` or generation-specific subcommand help before depending on newer commands.

## Dashboard management

Planned command namespace:

```text
belt account login
belt asteroid list
belt asteroid status <id>
belt asteroid create ...
belt asteroid update <id> ...
belt asteroid delete <id>
```

Dashboard endpoints remain undocumented. Commands ship only after request paths, authentication, CSRF behavior, response schemas, and lifecycle semantics are verified against authoritative dashboard traffic.
