# pardalote

pardalote (`pard`) is a minimal coding agent built for [shed](https://github.com/jirathip-dev/shed): shed starts
it and talks to it over the Agent Client Protocol (ACP), and pard does the model calls and the tool work of one
session.

## Non-goals

- No TUI: shed and its clients are the screen.
- No login code: shed injects the API key.
- No sandbox: shed wraps the process.
