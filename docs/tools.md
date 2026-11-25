# pard's tools

Six tools, compiled into `crates/pard-tools`, always in this order: `read`, `edit`, `write`, `shell`, `grep`,
`glob`. Their inputs and results are in [architecture/pard/shapes.md](architecture/pard/shapes.md), section 3.

- A relative path is under the session's folder; an absolute one is taken as written.
- Every result is cut once, when the tool makes it: 2,000 lines or 48 KiB. `read`, `grep` and `glob` keep the
  head, `shell` keeps the tail, and the cut says how to get the rest.
- A missing file, a bad regex, bad arguments or a non-zero exit is a failed result the model reads, never an
  error of the turn.
- `edit` replaces `old_string` exactly once, or everywhere with `replace_all`. With no exact match it tries once
  more with trailing spaces ignored and typographic quotes, dashes and spaces read as ASCII; that must find one
  place. A miss says `no match`, and where the text is with other indentation.
- `shell` runs `sh -c` in the session's folder in its own process group, without any provider's key variable.
  Its timeout is 120 s by default and at most 3,600 s. When the command ends or times out, or the call is
  dropped, the whole group is killed, so nothing it started keeps running.
- `grep` and `glob` skip what `.gitignore` and `.ignore` files skip (in a git repository or not), hidden files,
  and binary files. A `glob` pattern without a `/` matches a file name at any depth.
