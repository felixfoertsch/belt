# `belt` command model

## Local inventory

| Command | Effect |
|---|---|
| `belt add <name> <host>` | Detect Uberspace version over SSH, then add or replace local asteroid metadata |
| `belt add --all` | Merge all dashboard asteroids into local inventory and detect each version over SSH |
| `belt list` | List local asteroids available to remote commands |
| `belt remove <name>` | Remove local asteroid and cache |
| `belt import <inventory.json|inventory.yaml>` | Merge validated JSON or YAML inventory |
| `belt export [--json|--yaml]` | Export inventory; canonical format is JSON |
| `belt status [<name>] [--refresh] [--json]` | Show or refresh aggregate or single-asteroid status |

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

## Dashboard session

```text
belt login <username> [--password-stdin]
belt add --all
belt logout
```

Belt calls dashboard only for explicit dashboard operations. It persists only dashboard session cookie with mode `0600`. Dashboard second-factor authentication and asteroid lifecycle operations remain unsupported.
