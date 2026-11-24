# Contributing

## Run the gates locally

```sh
./.gatekeeper/run check
```

`just check` runs the same. It runs every CI gate through the pinned [gatekeeper](https://github.com/jirathip-dev/gatekeeper),
which fetches itself and its tools: its core and rust packs (fmt, clippy, the banned APIs, cargo-deny, the line cap,
arch-check, hygiene, actionlint) and pard's own gates in `.gatekeeper/gates/` (tests, docs, allows, root cause). One
gate runs alone with `--gate`, e.g. `./.gatekeeper/run check --gate rust/clippy`; `./.gatekeeper/run list` lists
them. The toolchain is the MSRV pinned in `rust-toolchain.toml`.

## Pull requests

- Open the PR against `staging`; fill in the template, including `## For humans` and the line count.
- New behaviour comes with a test that fails without it.
- Placeholders only in docs, tests and fixtures (`example.com`, `host-a`, `/home/user`).
- No AI co-author or "generated with" trailers.

By contributing you agree your work is licensed under MIT.
