# ADR: pard, a minimal ACP coding agent for shed (#2)

## Summary

Decision: pard is one small program that shed starts per lane. A core that knows no provider holds the ACP side, six fixed tools, an append-only session log and two generic model clients; each provider is one config file. The system prompt and tool list never change, so the provider's prompt cache keeps hitting.
Slices: (0) the dependencies alone; (1) conversation shapes and provider files; (2) transport, retries and the OpenAI-format client; (3) the Anthropic client; (4) the six tools; (5) the ACP side and the session log; (6) the turn loop, permissions and usage; (7) compaction and the cache checks. About 3,000 source lines. Then a rerun of the shed#375 task.
Owner decisions (2026-11-24, issue amendment): (1) no Claude subscription login: that provider file stays unwritten and Claude goes through an API key. (2) Keep the `agent-client-protocol` crate; its dependencies land in one dependency-only PR, slice 0, as the one exception to the 10-package gate. (3) One cap raise from 1,500 to 3,400 lines before slice 1. (4) Start the slices now.

## The question

shed runs agents only over ACP. For lanes on API-key models, shed#375 measured jcode, Pi and OpenCode on one
merged shed issue with `deepseek-flash`. Its verdict: "all three fall short on the gates". None passed
`just check`, one never ended its turn, and shed could count tokens for none of them
(`docs/trials/2026-11-api-agents.md` in shed). How should our own agent be built so that it is small, cheap to
run on such models, and fits shed without adapters?

## The options

| | option | why not, or why |
|---|---|---|
| A | Keep one of the three and patch around it. | Each failure in the trial is inside the agent: OpenCode's open turn and its writes to the real home, jcode's fallback to a computer-use tool, no token counts from any. We would own a fork of about 29 tools (jcode) or of an npm package and its adapter (Pi). |
| B | Our own agent with one wire format: every provider's Anthropic-format endpoint. | DeepSeek, Kimi and GLM each have one, so one client could do. But those endpoints are their second surface: as their docs read on 2026-11-24, DeepSeek's maps Claude model names onto its own and GLM documents its under the coding plan. The issue asks for both formats. |
| **C** | **Our own agent: a core with two generic clients, providers as files.** | Chosen. It is shed's own catalog pattern, and a new provider or price is a file, not a release of new code. |

## Decisions

### 1. The ACP side

- The `agent-client-protocol` crate at `=5.0.0`, shed's pin, with its `stdio` feature, as the issue says.
  Protocol version 1, the one shed asks for. The crate is most of pard's dependency tree: see decision 8 and
  the owner's decision 2.
- pard implements `initialize`, `session/new`, `session/load`, `session/prompt`, `session/cancel`,
  `session/set_config_option` (the `model` option) and `session/close`, and sends `session/update` and
  `session/request_permission`. The table of methods by module is in [README.md](README.md).
- Two places leave the ACP text on purpose, because of what shed does. `session/load` replays no history: shed
  drops a replay and shows its own transcript. `session/new` refuses MCP servers instead of ignoring them: pard
  has no MCP client in this version, and a lane that silently lost its tools would be worse than one that says
  so.
- Permission requests are the one hook shed and drover have on a tool call, so `edit`, `write` and `shell`
  always ask, with the model's raw arguments attached. `read`, `grep` and `glob` never ask.
- One pard process serves one or more sessions. One turn runs per session at a time; further prompts wait in
  order.

### 2. Model clients

- Two wire formats and no third: `openai` (chat completions) for DeepSeek, Kimi, GLM and OpenRouter, and
  `anthropic` (Messages). Each is a pure pair, `encode` and `decode`, with no network code, tested against
  recorded streams.
- Everything that differs between providers of one format is data in the provider file
  ([shapes.md](shapes.md), section 2): the URL, the auth header, extra headers, where the usage object keeps its
  cached-token count and cost, the error text that means "context full", whether earlier thinking must go back
  (DeepSeek's docs say a tool conversation without it is refused), and per model the context size, output
  limit, prices and extra request fields.
- **Streaming.** Always. Text and thinking go to shed as they arrive. A tool call runs only once its message is
  complete.
- **Retries.** A dropped connection, 408, 429, 500 to 504, 529, or an error event inside a stream: up to 6
  tries, waiting 1, 2, 4, 8, 16, 32 s or the provider's `retry-after`. Text already streamed stays on screen;
  the partial message is not logged.
- **Errors that end the turn.** 401 and 403 give `pard: auth: …`. A limit that outlasts the retries, or 402,
  gives `pard: limit: … resets <time>`. shed's record matches both prefixes, so a lane shows `error` with
  `auth:` or `rate-limited` as it does for Claude Code. A "context full" answer compacts once and calls again.
- **Requests carry no sampling settings and never force a tool.** Current Claude models refuse both.
- **Transport: a `curl` child process per model call.** The headers, the key among them, go to curl on its
  stdin; the request body is a 0600 file beside the session log. shed makes its own provider calls the same way.
  The other option is an HTTP crate with TLS. That keeps connections open between calls, and adds 86 lockfile
  packages (`reqwest` 0.12 with rustls, resolved on 2026-11-24) to a repo whose gate allows 10 a PR. curl costs
  one TLS handshake per call, which I have not measured. The transport is one function in `http`, so the rerun's
  wall time can change this decision in one file.

### 3. Tools

Six, compiled in: `read`, `edit`, `write`, `shell`, `grep`, `glob`. Their inputs, outputs and limits are in
[shapes.md](shapes.md), section 3. Pi's default set is four (`read`, `bash`, `edit`, `write`); jcode registers
about 29, and in the trial its shell broke and it reached for a computer-use tool.

**Edit format: exact string replacement, one edit per call** (`path`, `old_string`, `new_string`,
`replace_all`). What the four agents do, read on 2026-11-24 from Claude Code's docs and the other three's
sources:

| agent | tool | matching |
|---|---|---|
| Claude Code | `Edit{file_path, old_string, new_string, replace_all}` | Exact. One match unless `replace_all`. "It doesn't use regex or fuzzy matching." |
| OpenCode 2.4.1 | `edit{path, oldString, newString, replaceAll}`. A patch tool for GPT-5 and later only. | Exact, then typographic characters normalised, then trailing whitespace ignored. Version 1's chain of nine fuzzy matchers is gone. |
| Pi 1.6.0 | `edit{path, edits:[{oldText, newText}]}` | Exact, then one pass: Unicode forms, trailing whitespace, quotes, dashes. All edits or none. |
| jcode 1.1.2 | `edit{file_path, edits:[{old_string, new_string, replace_all}]}`, and a patch tool | Exact only, with hints on a miss. |

All four use string replacement for the models pard is for. The numbers agree:

- Aider's leaderboard, search-and-replace format: well-formed edits from DeepSeek V3 99.6 %, DeepSeek V3.2
  98.2 %, Kimi K2 92.9 %.
- "The Harness Problem" (2026-02, 16 models, 180 tasks): "patch is the worst format for nearly every model";
  GLM-4.7 failed 46.2 % of patches. Line-hash anchors beat replacement by 8.3 points on GLM-4.7 and 5 on Kimi
  K2.5, and lost by 8.3 on DeepSeek V3.2.

So: no patch format. No line-hash anchors: they lose on the first model pard targets and put a hash on every
line read. One flat edit per call, not an `edits` array: Pi's source notes that GLM-5.1 sends the array as a
string. Matching is exact, then the one tolerant pass OpenCode and Pi share, and it must find exactly one place.
`read` prints no line numbers, so text copied from a read is already a valid `old_string`.

The sources were read at the versions named, not run. The two sets of numbers are quoted from their pages;
newer models than those listed were not found there.

### 4. Sessions

An append-only log per session, one line per event, written before the step that needs it
([shapes.md](shapes.md), section 4). `session/load` after a hibernation, a crash or a kill rebuilds the same
conversation. Nothing is ever rewritten, compaction included. All of pard's state is under one folder,
`$PARD_HOME`: in the trial OpenCode wrote to four folders in the real home and jcode broke on a moved one.

### 5. Cache discipline

A request is, in order: the tool list, the system prompt, the session's preamble, the conversation.

- **Tool list and system prompt** are compiled in. They hold no date, folder, platform, model name or session
  detail. Together at most 8 KiB. A test compares the bytes of both wire formats' prefix with a file in the
  repo, so a PR that changes the prefix shows it as a diff.
- **The preamble** is the session's folder, platform and start date, the folder's `AGENTS.md` and
  `_meta.pard.instructions`. It is read once at `session/new`, stored in the log and never read again.
- **The conversation** only grows. Each item is re-sent as the bytes first sent; tool arguments are kept as the
  model wrote them. Tool output is cut to its cap once, when produced. Old output is never trimmed later, and
  nothing is inserted into earlier turns.
- **Anthropic format:** a cache mark after the fixed system prompt, one after the preamble, and the request-level
  automatic mark for the growing end. The other providers cache by prefix on their own.
- **Compaction.** When a call's context reaches 80 % of the model's window, or the provider says it is full, pard
  sends the conversation once more with a request to summarise it, which reads from the cache. From then on the
  conversation is that summary and the current prompt, word for word. Tool list, system prompt and preamble are
  untouched, so they still hit. No recent turns are kept verbatim: on current Claude models a kept turn's
  thinking blocks can be refused once the history before them has changed.
- **Target: at least 99.0 % cache hits** over a session on the shed#375 task, counted as shed's report counts
  it. The three agents reached 98.0 to 99.3 %. At `deepseek-flash`'s prices (US$0.15 uncached, 0.003 cached and
  0.60 output per million, which reproduce OpenCode's reported US$0.146 from its token counts) a cached token
  costs a fiftieth of an uncached one. On Pi's run one point of hit rate is about US$0.014, a sixth of the whole
  run.
- What still loses the cache is outside pard: a model switch (shed warns), and a hibernation longer than the
  provider keeps its cache.

### 6. The shed contract

- **The key** arrives in the variable the provider file names as `key_env`, the `token_var` of shed's record.
  pard reads it from the environment and stores it nowhere. The `shell` tool's commands run without any
  provider's key variable.
- **Usage, cache and cost** go out in the two places shed reads: `usage_update` with the context size and the
  session's cost, and the turn's four token counts in the prompt response's `_meta`
  ([shapes.md](shapes.md), section 6). pard prices each call from the provider file, or takes the provider's own
  cost where it reports one.
- **Sessions** are on disk and `loadSession` is advertised, so shed can hibernate and resume a lane.
- **No TUI, no login code, no sandbox.** pard draws nothing, signs in nowhere and fences no path. A turn ends
  with no process of pard's alive, so a lane is never left `running` after its turn.
- One gap: DeepSeek charges double at peak hours and a provider file has one price. pard's cost for that
  provider is the off-peak one; the balance shed reads is the truth.

### 7. A Claude subscription login

The check the amendment asked for, read on 2026-11-24:

- Consumer Terms, effective 2025-10-08 (<https://www.anthropic.com/legal/consumer-terms>): a user may not
  "access the Services through automated or non-human means", "Except when you are accessing our Services via an
  Anthropic API Key" or where Anthropic explicitly permits it.
- Usage Policy, effective 2026-11-12 (<https://www.anthropic.com/legal/aup>): do not "Resell, proxy, or otherwise
  provide access to Claude through unauthorized means", "including services that route requests through consumer
  subscriptions or misrepresent the product or client being used".
- Claude Code's legal page (<https://code.claude.com/docs/en/legal-and-compliance>): OAuth "is designed to
  support ordinary use of Claude Code and other native Anthropic applications", and "Anthropic does not permit
  third-party developers to offer Claude.ai login into their own applications, or to route requests through
  Free, Pro, Max, or Ultra plan credentials on behalf of their users." It "may do so without prior notice" when
  enforcing. What it leaves open is "signing in to the unmodified Claude Code binary with their own Claude
  subscription".

**Finding: not permitted.** jcode 1.1.2's route (`crates/jcode-base/src/auth/oauth.rs`,
`crates/jcode-provider-anthropic/src/lib.rs`) signs in with what its source calls the "Claude Code OAuth
configuration" and puts Claude Code's billing attribution text in each request; its source records that a
request with neither that text nor an Agent SDK identity line is refused. That is the "misrepresent the product
or client" case, and pard is a public repo, so shipping it is also the "third-party developers" case. The risk
is the owner's Claude account: the terms allow suspension without notice.

**Decision.** The design keeps it a separate provider file, as decided: it would be one optional file with a
bearer `auth` line, extra `headers` and one new key for a fixed prefix on the system prompt, built in only under
a Cargo feature that is off by default, and needing no login code, since shed already hands over the setup
token. No slice writes that file or that key. The owner decided it stays unwritten. Two permitted routes cover the
need: Claude through the `anthropic` provider file with an API key, and subscription work in shed's existing
`claude-acp` lanes, which run Claude Code itself.

### 8. Sizing, the cap and the success bar

About 3,000 non-test source lines for a first usable version, by module in [README.md](README.md). `line_cap` is
1,500 today. One raise to 3,400 (the estimate plus 13 %) in a `cap:` issue before slice 1.

Dependencies, all new to this repo: `agent-client-protocol` (the ACP side), `tokio` (the runtime, child
processes, timers), `serde`, `serde_json`, `toml` (provider files), `regex` and `ignore` (`grep`, `glob`), `uuid`
(session ids; in the tree already through the ACP crate), and `futures` only if the ACP crate's handlers need
it. Versions are pinned exactly, at shed's where shed has the crate.

What that tree weighs, resolved in a scratch folder on 2026-11-24 and counted as lockfile packages:

| set | packages |
|---|---|
| `tokio`, `serde`, `serde_json`, `toml` | 31 |
| those, `regex` and `ignore` | 45 |
| those and `agent-client-protocol` with `stdio` | 153 |
| those and `reqwest` with rustls | 239 |

`.gatekeeper/deps.toml` allows a PR to add 10 packages, and `deny.toml` denies two versions of one package. A
fresh resolve of the 153 lists two versions of nine packages in the lockfile; the 45 without the ACP crate list
none. shed builds the same crate under the same rule with one `skip` in its `deny.toml` (`syn` 2, for the ACP
crate's macros), so slice 0 starts from shed's lockfile versions and copies that line. Slice 0 is that one PR: the dependencies and nothing else, so the owner's exception covers one reviewable diff. The
rejected way out (owner decision 2): no ACP crate, the nine messages pard uses written by hand over
`serde_json`. That is about 300 more lines in `acp`, a cap of 3,700, a tree of 45 packages, and it departs from
the issue's text.

The bar, from shed#375's table. Same task, model and clean room; three runs, the median counts:

| | jcode | Pi | OpenCode | pard |
|---|---|---|---|---|
| `just check` | fail | fail | fail | pass, all 32 gates |
| merged PR's tests | 2 of 4 | 3 of 4 | 4 of 4 | 4 of 4 |
| turn finished | yes | yes | no | yes |
| cost | US$0.14 | US$0.08 | US$0.15 | at most US$0.08 |
| cache hit | 99.3 % | 98.0 % | 99.2 % | at least 99.0 % |
| wall time | 785 s | 1,207 s | 3,622 s | at most 1,207 s |

The first three rows are the bar. The trial ranked the gates first, and no agent met all three. A run that meets
them and misses on cost or time is reported to the owner with the numbers.

## Consequences

- **It is a bet on the gates.** The same model failed `just check` in three agents. pard's levers are a system
  prompt that says to find and run the folder's check command before finishing, a shell that can run a
  ten-minute check, and tools that do not break. Nothing in the design makes a model pass. If the rerun fails the
  same gates, the next step is a check command that pard itself runs at the end of a turn and feeds back, which
  is a new decision.
- **No MCP, images, plans, slash commands, sub-agents or web tools** in this version. A lane that needs one runs
  on another agent.
- **Adding a provider or a price is a file.** Adding a third wire format, or a behaviour two providers of one
  format cannot share as data, is code and a new decision.
- **Changing the system prompt or a tool is visible and costs every live session its cache once.** That is the
  point of the prefix test.
- **A compaction forgets detail.** Only the summary and the current prompt survive it. With 1M-token windows it
  should be rare; on a 200k model it is not.
- **pard needs `curl` on the host**, as shed does.
- **The trial's sandbox finding stands.** With `--policy allow`, pard reads and runs whatever the model asks.
  The fence is shed's.

## Hot files and `arch.toml`

Nothing here is hot yet. Two places would become so, and the layout avoids both:

- **The provider list** is a folder, `crates/pard-model/providers/`, one file per provider, compiled in by the
  crate's `build.rs`. There is no list in code to edit.
- **Docs** are one file per area, each written by the slice that owns it: `docs/providers.md`, `docs/tools.md`,
  `docs/sessions.md`, `docs/cache.md`.

The root `Cargo.toml` and `.gatekeeper/deps.toml` get every dependency in slice 0, so later slices do not meet
in them. The tool list stays one list in `pard-tools` on purpose: it is part of the cached prefix and the prefix
test guards it.

Slice 1 adds to `.drover/arch.toml`:

```toml
[[collect]]
dir = "crates/pard-model/providers"
glob = "*.toml"
```
