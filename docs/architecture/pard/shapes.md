# pard: data shapes

Types are sketches for the impl lane, not product code. Field names are the decision; exact derives are not.

## 1. The conversation (`pard-model`)

One conversation shape for both wire formats. It is what the session log stores and what each request re-sends.

```rust
pub enum Item {
    User(String),
    Assistant(Vec<Block>),
    Results(Vec<ToolResult>),
}

pub enum Block {
    Thinking { text: String, signature: Option<String> },
    Text(String),
    Call(Call),
}

pub struct Call {
    pub id: String,
    pub tool: String,
    pub input: Box<RawValue>,
}

pub struct ToolResult {
    pub id: String,
    pub text: String,
    pub failed: bool,
}

pub struct Usage {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}
```

- `Block`s keep the order the model sent them. A thinking block goes back unchanged, signature included, when the
  provider file says `reasoning = "echo"`.
- `Call::input` is the argument text exactly as the model sent it, never parsed and written again. Re-sending the
  same bytes is what keeps the provider's cache prefix valid.
- `Call::tool` is a string because a model can name a tool that does not exist. The answer is a failed result,
  not a broken log.
- `Usage::input` is uncached input only, as shed counts it. The OpenAI wire subtracts the cached tokens from
  `prompt_tokens`.

What a model call gives back:

```rust
pub enum Delta { Thinking(String), Text(String) }

pub enum Stop { End, Calls, MaxTokens, Refusal }

pub struct Answer {
    pub blocks: Vec<Block>,
    pub stop: Stop,
    pub usage: Usage,
    pub cost: Option<Money>,
}

pub enum ModelError {
    Auth { provider: String, status: u16 },
    Limit { provider: String, status: u16, resets: Option<String> },
    Overflow,
    Fatal(String),
    Cancelled,
}
```

`Delta`s stream while the call runs. Tool calls arrive whole in the `Answer`, so a half-streamed call never runs.

## 2. A provider file

TOML, one file per provider. Built-ins live in `crates/pard-model/providers/*.toml` and are compiled in. A file
of the same name in `$PARD_HOME/providers/` overrides a built-in key by key, or adds a provider. The core has no
provider name in its code.

```toml
name = "DeepSeek"
wire = "openai"
base_url = "https://api.deepseek.com"
key_env = "DEEPSEEK_API_KEY"
auth = "authorization: Bearer {key}"
reasoning = "echo"

[usage]
cache_read = ["prompt_cache_hit_tokens"]

[errors]
overflow = ["maximum context length"]

[[models]]
id = "deepseek-flash"
name = "DeepSeek V4.1 Flash"
context = 1000000
max_output = 32000

[models.price]
currency = "USD"
input = 0.15
cache_read = 0.003
cache_write = 0.15
output = 0.60
```

These are DeepSeek's off-peak prices as its price page showed them on 2026-11-24. The slice that adds a provider
file reads the context size and prices from the provider's own page again and names the page in the PR.

| key | meaning |
|---|---|
| `wire` | `openai` (chat completions) or `anthropic` (Messages). The only two the core has. |
| `base_url` | The wire adds `/chat/completions` or `/v1/messages`. |
| `key_env` | The variable the key arrives in. It is the `token_var` of shed's record. |
| `auth` | The header that carries the key. `{key}` stands for it. |
| `headers` | A table of fixed extra headers, e.g. `anthropic-version`. |
| `reasoning` | `echo`: earlier thinking goes back to the model. `drop` (default): it is stored and not re-sent. |
| `[cache]` | `mode = "auto"` (default: the provider caches by prefix, pard sends nothing) or `"marked"` (the Anthropic wire's `cache_control`); `ttl`, `"5m"` or `"1h"`, for `marked`. |
| `[usage]` | Key paths into the response's usage object for `cache_read`, `cache_write` and `cost`, where a provider's names differ from the wire's own. Same path form as shed's `[balance]`. |
| `[errors] overflow` | A 400 whose body holds one of these means the context is full. |
| `[[models]]` | `id`, `name`, `context`, `max_output`, `[models.price]` (per million tokens, shed's `[price]` keys) and `body`, a table merged into every request for that model (an effort level, say). |

A model's id on the ACP side is `<file name>/<model id>`, e.g. `deepseek/deepseek-flash`.

## 3. The six tools

Fixed names, fixed order, fixed schemas, compiled in. No provider, session or config changes them.

| tool | input | result |
|---|---|---|
| `read` | `path`, `offset?` (first line, from 1), `limit?` (lines) | The text as it is in the file, no line numbers. |
| `edit` | `path`, `old_string`, `new_string`, `replace_all?` | A one-line confirmation. |
| `write` | `path`, `content` | A one-line confirmation. Makes parent folders. |
| `shell` | `command`, `timeout?` (seconds, default 120, at most 3600) | Exit code, then stdout and stderr together. |
| `grep` | `pattern` (regex), `path?`, `glob?` | `path:line:text`, in path order. |
| `glob` | `pattern`, `path?` | Paths, newest first. |

Rules that hold for all six:

- A relative `path` is under the session's folder. An absolute one is taken as written: shed's sandbox is the
  fence, pard has none.
- One output cap: 2,000 lines or 48 KiB. `read`, `grep` and `glob` keep the head, `shell` keeps the tail. A cut
  result says what was cut and how to get the rest (`offset`, a narrower pattern).
- `grep` and `glob` skip what the folder's ignore files skip, and binary files.
- A tool never fails the turn. A missing file, a bad regex or a non-zero exit is a `ToolResult` with
  `failed: true` and a message the model can act on.

`edit` matching, in this order:

1. `old_string` exactly. One match, or any number with `replace_all`.
2. If there is none: the same search with trailing whitespace per line ignored and typographic quotes, dashes and
   spaces read as their ASCII forms. It must give exactly one match.
3. Else a failed result that names the cause: `no match`, with `found with different indentation near line N`
   when that is so, or `N matches: add lines around it or set replace_all`.

`shell` runs `sh -c <command>` in the session's folder, in its own process group, with every provider's
`key_env` variable removed from its environment. When the command returns, times out or the turn is cancelled,
the whole group is killed. Nothing runs in the background.

## 4. The session log

`$PARD_HOME/sessions/<session id>.jsonl`, mode 0600 in a 0700 folder. Append-only, one JSON object per line,
written and flushed before the step that depends on it. `session/load` is a fold over the lines.

| `t` | fields | written when |
|---|---|---|
| `start` | `v`, `id`, `cwd`, `model`, `preamble`, `at` | `session/new`. Never written again. |
| `item` | `item` (an `Item`) | A prompt arrives, a model call ends, a message's tool calls all have results. |
| `usage` | `model`, the four counts, `cost` | After each model call. |
| `model` | `model` | `session/set_config_option`. |
| `grant` | `tool` | A permission answered "always allow". |
| `compact` | `summary`, `request` | Compaction. Items before this line are no longer sent. |

Loading repairs two things and nothing else: a last line that is not whole JSON is dropped, and a `Call` with no
result gets one saying `interrupted`. Both leave the file a valid conversation, so loading twice gives the same
session. Logs untouched for 30 days are deleted when pard starts.

## 5. What the turn loop says to its caller (`pard`, `turn.rs`)

The loop knows no ACP type. `acp.rs` implements this trait; tests implement it with a recorder.

```rust
pub enum Event {
    Delta(Delta),
    CallStarted { id: String, kind: Kind, title: String, path: Option<PathBuf>, input: Box<RawValue> },
    CallEnded { id: String, failed: bool, output: Output },
    Usage { used: u64, size: u64, cost: Option<Money> },
}

pub enum Kind { Read, Edit, Execute, Search }

pub enum Output { Text(String), Diff { path: PathBuf, old: String, new: String } }

pub enum Permit { Once, Always, Rejected, Cancelled }

pub trait Client {
    fn event(&mut self, event: Event);
    async fn permit(&mut self, id: &str, tool: &str) -> Permit;
}

pub enum TurnEnd { EndTurn, MaxTokens, MaxTurnRequests, Refusal, Cancelled }
```

## 6. What shed reads

| shed reads | pard sends |
|---|---|
| `usage_update` (`TOKENS` in `shed top`, the account's month cost) | After each model call: `used`, the tokens of that call's whole context; `size`, the model's `context`; `cost`, the session's total so far, kept across a hibernation. |
| The prompt response's `_meta` (`CACHE` in `shed top`, the month's tokens) | `{"pard": {"turn": {"input": n, "cache_read": n, "cache_write": n, "output": n}}}`, the turn's sum. |
| The stop reason | `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`. |
| A login failure | The prompt's error message starts `pard: auth:`. |
| A limit | It starts `pard: limit:`, with `resets <time>` when the provider gave a retry time. |

The record shed needs for pard on a DeepSeek account. It is shed's file, a follow-up there, not part of pard:

```toml
id = "pard"
agent = "deepseek"
token_var = "DEEPSEEK_API_KEY"
model_env = "PARD_MODEL"
models = [["deepseek/deepseek-flash", "DeepSeek V4.1 Flash"]]
auth_failures = ["pard: auth:"]
limit = ["pard: limit:"]
resets = ["resets "]

[quota]
path = ["pard", "turn"]
input = "input"
cache_read = "cache_read"
cache_write = "cache_write"
output = "output"

[sandbox]
write = ["~/.local/share/pard"]
hosts = ["api.deepseek.com"]
```

Environment pard reads, and nothing else: each provider's `key_env`; `PARD_MODEL` (`<provider>/<model>`, the
session's first model); `PARD_HOME` (default `~/.local/share/pard`), the one folder pard writes outside the
session's own.
