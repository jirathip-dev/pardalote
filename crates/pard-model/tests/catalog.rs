//! The provider catalog: built-ins, user overrides and the errors a bad file gives.

use std::fs;
use std::path::PathBuf;

use pard_model::catalog::{CacheMode, Catalog, CatalogError, Price, Reasoning, Wire};

fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("catalog")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn error(name: &str, files: &[(&str, &str)]) -> String {
    Catalog::load(&folder(name, files)).unwrap_err().to_string()
}

#[test]
fn the_five_built_in_providers_load_with_no_user_folder() {
    let catalog = Catalog::load(&folder("none", &[]).join("missing")).unwrap();
    let ids: Vec<&str> = catalog.providers().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["anthropic", "deepseek", "glm", "kimi", "openrouter"]);

    let (deepseek, flash) = catalog.model("deepseek/deepseek-flash").unwrap();
    assert_eq!(deepseek.wire, Wire::Openai);
    assert_eq!(deepseek.reasoning, Reasoning::Echo);
    assert_eq!(
        deepseek.usage.cache_read.as_deref(),
        Some(&["prompt_cache_hit_tokens".to_owned()][..])
    );
    assert_eq!(flash.context, 1_000_000);
    let price = Price {
        currency: "USD".to_owned(),
        input: 0.15,
        cache_read: 0.003,
        cache_write: 0.15,
        output: 0.60,
    };
    assert_eq!(flash.price, Some(price));

    let (anthropic, _) = catalog.model("anthropic/claude-sonnet-5-5").unwrap();
    assert_eq!(anthropic.wire, Wire::Anthropic);
    assert_eq!(anthropic.cache.mode, CacheMode::Marked);
    assert_eq!(anthropic.auth, "x-api-key: {key}");
    assert_eq!(anthropic.key_env, "ANTHROPIC_API_KEY");

    let (openrouter, model) = catalog
        .model("openrouter/deepseek/deepseek-v4.1-flash-1017")
        .unwrap();
    assert_eq!(openrouter.id, "openrouter");
    assert_eq!(model.id, "deepseek/deepseek-v4.1-flash-1017");
}

#[test]
fn every_provider_takes_an_api_key_from_its_own_variable() {
    let catalog = Catalog::load(&folder("keys", &[])).unwrap();
    for provider in catalog.providers() {
        assert!(provider.key_env.ends_with("_API_KEY"), "{}", provider.id);
        assert!(
            !provider.auth.to_lowercase().contains("oauth"),
            "{}",
            provider.id
        );
    }
}

#[test]
fn a_subscription_prefix_key_is_refused() {
    let text = "system_prefix = \"You are Claude Code\"\n";
    let message = error("prefix", &[("anthropic.toml", text)]);
    assert!(
        message.contains("anthropic.toml") && message.contains("system_prefix"),
        "{message}"
    );
}

#[test]
fn a_user_file_overrides_a_built_in_key_by_key() {
    let dir = folder(
        "override",
        &[(
            "deepseek.toml",
            "base_url = \"http://127.0.0.1:9\"\n[usage]\ncost = [\"cost\"]\n",
        )],
    );
    let catalog = Catalog::load(&dir).unwrap();
    let (provider, model) = catalog.model("deepseek/deepseek-flash").unwrap();
    assert_eq!(provider.base_url, "http://127.0.0.1:9");
    assert_eq!(
        provider.usage.cost.as_deref(),
        Some(&["cost".to_owned()][..])
    );
    assert_eq!(
        provider.usage.cache_read.as_deref(),
        Some(&["prompt_cache_hit_tokens".to_owned()][..])
    );
    assert_eq!(provider.key_env, "DEEPSEEK_API_KEY");
    assert_eq!(model.name, "DeepSeek V4.1 Flash");
}

#[test]
fn a_user_file_adds_a_provider() {
    let text = "name = \"Example\"\nwire = \"openai\"\nbase_url = \"https://api.example.com/v1\"\n\
        key_env = \"EXAMPLE_API_KEY\"\nauth = \"authorization: Bearer {key}\"\n\
        [[models]]\nid = \"example-large\"\nname = \"Example Large\"\ncontext = 200000\nmax_output = 16000\n\
        [models.body]\nreasoning_effort = \"high\"\n";
    let dir = folder(
        "add",
        &[("example.toml", text), ("notes.txt", "not a provider")],
    );
    let catalog = Catalog::load(&dir).unwrap();
    assert_eq!(catalog.providers().count(), 6);
    let (provider, model) = catalog.model("example/example-large").unwrap();
    assert_eq!(provider.reasoning, Reasoning::Drop);
    assert_eq!(provider.cache.mode, CacheMode::Auto);
    assert!(model.price.is_none());
    assert_eq!(model.body["reasoning_effort"], "high");
}

#[test]
fn a_malformed_file_names_itself_and_the_fault() {
    let message = error("syntax", &[("glm.toml", "base_url = \n")]);
    assert!(
        message.starts_with("provider file ") && message.contains("glm.toml"),
        "{message}"
    );

    let message = error("wire", &[("glm.toml", "wire = \"grpc\"\n")]);
    assert!(
        message.contains("glm.toml") && message.contains("grpc"),
        "{message}"
    );

    let message = error(
        "auth",
        &[("glm.toml", "auth = \"authorization: Bearer secret\"\n")],
    );
    assert!(message.contains("{key}"), "{message}");

    let message = error(
        "partial",
        &[("new.toml", "name = \"New\"\nwire = \"openai\"\n")],
    );
    assert!(
        message.contains("new.toml") && message.contains("base_url"),
        "{message}"
    );

    let message = error("empty", &[("kimi.toml", "models = []\n")]);
    assert!(
        message.contains("kimi.toml") && message.contains("no [[models]]"),
        "{message}"
    );

    let twice = "[[models]]\nid = \"a\"\nname = \"A\"\ncontext = 1\nmax_output = 1\n\
        [[models]]\nid = \"a\"\nname = \"A\"\ncontext = 1\nmax_output = 1\n";
    let message = error("twice", &[("kimi.toml", twice)]);
    assert!(message.contains("model a is listed twice"), "{message}");
}

#[test]
fn a_user_folder_that_is_a_file_is_an_error() {
    let dir = folder("file", &[("providers", "")]);
    let err = Catalog::load(&dir.join("providers")).unwrap_err();
    assert!(matches!(err, CatalogError::Read { .. }), "{err}");
}

#[test]
fn an_unknown_model_or_provider_is_an_error() {
    let catalog = Catalog::load(&folder("unknown", &[])).unwrap();
    for id in ["nope/x", "deepseek/nope", "deepseek-flash", ""] {
        let message = catalog.model(id).unwrap_err().to_string();
        assert_eq!(
            message,
            format!("unknown model {id}: expected <provider file>/<model id>")
        );
    }
}
