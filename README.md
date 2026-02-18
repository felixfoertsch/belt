# uberspace-cli (experimental)

This is a Go CLI scaffold intended to work against the Uberspace dashboard by replaying the same HTTP calls your browser makes. It does not include hardcoded endpoints because there is no public API.

## Quick start

1. Copy `uberspace-cli.example.yaml` to your config path.
2. Update the endpoint paths, headers, and bodies to match real dashboard requests.
3. Run `go build ./cmd/uberspace-cli`.
4. Use `./uberspace-cli login` to save a session, then `create-asteroid` or `add-ssh-key`.

## Capture requests

Use your browser devtools network tab on the Uberspace dashboard.

1. Log in and create an asteroid in the UI.
2. Find the request in the network list.
3. Copy method, path, required headers, and payload into the config.
4. If there is a CSRF cookie or header, configure `csrf` in `login`.

## Example commands

```bash
./uberspace-cli login --email you@example.com --password '...'
./uberspace-cli create-asteroid --name my-asteroid
./uberspace-cli add-ssh-key --name laptop --public-key-file ~/.ssh/id_ed25519.pub
```

## Notes

This is unofficial and may break if the dashboard changes.
