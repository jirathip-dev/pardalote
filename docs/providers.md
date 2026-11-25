# Providers

A provider is one TOML file. The built-ins are in `crates/pard-model/providers/` and are compiled into pard:

| file | provider | key variable | wire |
|---|---|---|---|
| `deepseek.toml` | DeepSeek | `DEEPSEEK_API_KEY` | `openai` |
| `kimi.toml` | Kimi (Moonshot) | `MOONSHOT_API_KEY` | `openai` |
| `glm.toml` | Z.ai GLM | `ZAI_API_KEY` | `openai` |
| `openrouter.toml` | OpenRouter | `OPENROUTER_API_KEY` | `openai` |
| `anthropic.toml` | Anthropic | `ANTHROPIC_API_KEY` | `anthropic` |

Claude is reached only through `anthropic.toml` and an API key. pard has no subscription or OAuth login.

A file of the same name in `$PARD_HOME/providers/` overrides a built-in key by key (tables merge, other values
and the `[[models]]` list replace); a new name adds a provider. A file that does not parse, names an unknown key,
lacks a required one, has no `{key}` in `auth`, no `[[models]]` or one model id twice stops pard with a message
that names the file.

A model's id is `<file name>/<model id>`, e.g. `deepseek/deepseek-flash`. The keys are in
[the ADR's shapes](architecture/pard/shapes.md), section 2.
