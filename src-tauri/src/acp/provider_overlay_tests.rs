use super::*;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::registry;
use std::fs;

#[test]
fn every_agent_receives_exact_model_gateway_root() {
    for agent_type in registry::all_acp_agents() {
        let mut env = BTreeMap::new();
        apply_provider_runtime_env_with_base(agent_type, &mut env, MODEL_GATEWAY_BASE_URL);
        let value = env
            .get(provider_base_url_env_key(agent_type))
            .expect("provider base URL");
        assert_eq!(value, MODEL_GATEWAY_BASE_URL, "{agent_type:?}");
        assert!(!value.ends_with("/v1"), "{agent_type:?}");
        assert!(!value.ends_with("/anthropic"), "{agent_type:?}");
    }
}

#[test]
fn runtime_overlay_replaces_stale_provider_url_only() {
    let mut env = BTreeMap::from([
        (
            "ANTHROPIC_BASE_URL".to_string(),
            "https://old/anthropic".to_string(),
        ),
        ("UNRELATED".to_string(), "keep".to_string()),
    ]);

    apply_provider_runtime_env_with_base(AgentType::ClaudeCode, &mut env, MODEL_GATEWAY_BASE_URL);

    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").map(String::as_str),
        Some(MODEL_GATEWAY_BASE_URL)
    );
    assert_eq!(env.get("UNRELATED").map(String::as_str), Some("keep"));
}

#[test]
fn custom_model_gateway_replaces_the_whole_url_without_suffixes() {
    let custom = "https://gateway.example/custom/root";
    for agent_type in registry::all_acp_agents() {
        let mut env = BTreeMap::new();
        apply_provider_runtime_env_with_base(agent_type, &mut env, custom);
        assert_eq!(
            env.get(provider_base_url_env_key(agent_type))
                .map(String::as_str),
            Some(custom)
        );
    }
}

#[test]
fn codex_toml_overlay_forces_managed_provider_and_preserves_unrelated_sections() {
    let raw = r#"
model = "keep-model"
model_provider = "old"
[mcp_servers.demo]
command = "demo"
[model_providers.old]
base_url = "https://old.example/v1"
custom = "keep"
"#;

    let patched = patch_codex_toml(raw, MODEL_GATEWAY_BASE_URL).expect("patch codex");
    let value = patched.parse::<toml::Value>().expect("valid toml");
    assert_eq!(value["model"].as_str(), Some(MANAGED_DEFAULT_MODEL));
    assert_eq!(value["model_provider"].as_str(), Some("iyw-claw"));
    assert_eq!(
        value["model_providers"]["iyw-claw"]["base_url"].as_str(),
        Some(MODEL_GATEWAY_BASE_URL)
    );
    assert!(value["model_providers"].get("old").is_none());
    assert_eq!(
        value["mcp_servers"]["demo"]["command"].as_str(),
        Some("demo")
    );
}

#[test]
fn json_overlays_preserve_permissions_skills_and_custom_fields() {
    let source = serde_json::json!({
        "permissions": {"allow": ["Read"]},
        "skills": {"demo": true},
        "custom": {"keep": 1}
    });
    for agent in [
        AgentType::ClaudeCode,
        AgentType::Gemini,
        AgentType::OpenCode,
        AgentType::OpenClaw,
        AgentType::Cline,
        AgentType::CodeBuddy,
        AgentType::Pi,
    ] {
        let patched =
            patch_json_config(agent, source.clone(), MODEL_GATEWAY_BASE_URL).expect("patch json");
        assert_eq!(patched["permissions"], source["permissions"], "{agent:?}");
        assert_eq!(patched["skills"], source["skills"], "{agent:?}");
        assert_eq!(patched["custom"], source["custom"], "{agent:?}");
        assert_json_provider_overlay(agent, &patched);
    }
}

#[test]
fn hermes_yaml_overlay_forces_managed_model_and_preserves_tools() {
    let raw = r#"
model:
  provider: openrouter
  default: keep-model
  base_url: https://old.example/v1
tools:
  enabled: true
"#;
    let patched = patch_hermes_yaml(raw, MODEL_GATEWAY_BASE_URL).expect("patch hermes");
    let value: serde_yaml::Value = serde_yaml::from_str(&patched).expect("valid yaml");
    assert_eq!(value["model"]["provider"].as_str(), Some("custom"));
    assert_eq!(
        value["model"]["default"].as_str(),
        Some(MANAGED_DEFAULT_MODEL)
    );
    assert_eq!(
        value["model"]["base_url"].as_str(),
        Some(MODEL_GATEWAY_BASE_URL)
    );
    assert_eq!(value["tools"]["enabled"].as_bool(), Some(true));
}

#[test]
fn kimi_toml_overlay_forces_managed_models_and_preserves_custom_tables() {
    let raw = r#"
default_model = "keep-alias"
[models.keep-alias]
provider = "old"
model = "keep-model"
max_context_size = 12345
[custom]
value = "keep"
"#;
    let patched = patch_kimi_toml(raw, MODEL_GATEWAY_BASE_URL).expect("patch kimi");
    let value = patched.parse::<toml::Value>().expect("valid toml");
    assert_eq!(value["default_model"].as_str(), Some(MANAGED_DEFAULT_MODEL));
    assert_eq!(
        value["models"][MANAGED_DEFAULT_MODEL]["provider"].as_str(),
        Some("iyw-claw")
    );
    assert_eq!(
        value["models"][MANAGED_DEFAULT_MODEL]["model"].as_str(),
        Some(MANAGED_DEFAULT_MODEL)
    );
    assert_eq!(
        value["models"].as_table().map(toml::map::Map::len),
        Some(MANAGED_MODEL_IDS.len())
    );
    assert_eq!(
        value["providers"]["iyw-claw"]["base_url"].as_str(),
        Some(MODEL_GATEWAY_BASE_URL)
    );
    assert_eq!(value["custom"]["value"].as_str(), Some("keep"));
}

#[test]
fn filesystem_overlay_writes_private_files_and_preserves_auth() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let codex = paths.profile(AgentType::Codex).root;
    fs::create_dir_all(&codex).expect("codex dir");
    fs::write(codex.join("config.toml"), "model = \"keep\"\n").expect("config");
    fs::write(codex.join("auth.json"), "{\"token\":\"keep\"}\n").expect("auth");

    enforce_provider_overlay(AgentType::Codex, &paths).expect("enforce codex");

    let config = fs::read_to_string(codex.join("config.toml")).expect("read config");
    let value = config.parse::<toml::Value>().expect("toml");
    assert_eq!(value["model"].as_str(), Some(MANAGED_DEFAULT_MODEL));
    assert_eq!(value["model_provider"].as_str(), Some("iyw-claw"));
    assert_eq!(
        fs::read_to_string(codex.join("auth.json")).expect("read auth"),
        "{\"token\":\"keep\"}\n"
    );

    let hermes = paths.profile(AgentType::Hermes).root;
    fs::create_dir_all(&hermes).expect("hermes dir");
    fs::write(hermes.join(".env"), "OPENAI_API_KEY=keep-secret\nKEEP=1\n").expect("hermes env");

    enforce_provider_overlay(AgentType::Hermes, &paths).expect("enforce hermes");

    let env = fs::read_to_string(hermes.join(".env")).expect("read hermes env");
    assert!(env.contains("OPENAI_API_KEY=keep-secret\n"));
    assert!(env.contains("KEEP=1\n"));
    assert!(env.contains(&format!("OPENAI_BASE_URL={MODEL_GATEWAY_BASE_URL}\n")));
}

#[test]
fn filesystem_overlay_covers_every_private_profile() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    for agent in registry::all_acp_agents() {
        fs::create_dir_all(paths.profile(agent).root).expect("profile dir");
    }

    enforce_all_provider_overlays(&paths).expect("enforce all");

    assert!(paths
        .profile(AgentType::Codex)
        .root
        .join("config.toml")
        .is_file());
    assert!(paths
        .profile(AgentType::Hermes)
        .root
        .join("config.yaml")
        .is_file());
    assert!(paths.profile(AgentType::Hermes).root.join(".env").is_file());
    assert!(paths
        .profile(AgentType::Cline)
        .root
        .join("globalState.json")
        .is_file());
    assert!(paths
        .profile(AgentType::Pi)
        .root
        .join("settings.json")
        .is_file());
    assert!(paths
        .profile(AgentType::Pi)
        .root
        .join("models.json")
        .is_file());
}

#[test]
fn startup_overlay_does_not_create_missing_profiles() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));

    enforce_existing_provider_overlays(&paths).expect("repair existing profiles");

    assert!(!paths.config_dir().exists());
}

#[test]
fn startup_overlay_repairs_only_existing_profiles() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let codex = paths.profile(AgentType::Codex).root;
    fs::create_dir_all(&codex).expect("codex profile");
    fs::write(codex.join("config.toml"), "model = \"keep\"\n").expect("codex config");

    enforce_existing_provider_overlays(&paths).expect("repair existing profiles");

    assert!(codex.join("config.toml").is_file());
    assert!(!paths.profile(AgentType::ClaudeCode).root.exists());
    assert!(!paths.profile(AgentType::Gemini).root.exists());
    assert!(!paths.profile(AgentType::OpenCode).root.exists());
}

fn assert_json_provider_overlay(agent: AgentType, value: &serde_json::Value) {
    match agent {
        AgentType::ClaudeCode | AgentType::CodeBuddy => {
            assert_eq!(
                value["env"]["ANTHROPIC_BASE_URL"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
        }
        AgentType::Gemini => {
            assert_eq!(
                value["env"]["GOOGLE_GEMINI_BASE_URL"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
        }
        AgentType::OpenCode => {
            assert_eq!(
                value["provider"]["iyw-claw"]["options"]["baseURL"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
        }
        AgentType::OpenClaw => {
            assert_eq!(
                value["models"]["providers"]["iyw-claw"]["baseUrl"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
        }
        AgentType::Cline => {
            assert_eq!(value["actModeApiProvider"].as_str(), Some("openai"));
            assert_eq!(value["planModeApiProvider"].as_str(), Some("openai"));
            assert_eq!(
                value["openAiBaseUrl"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
        }
        AgentType::Pi => {
            assert_eq!(value["defaultProvider"].as_str(), Some("iyw-claw"));
            let models = patch_pi_models_json(
                serde_json::json!({"custom": 1}),
                MODEL_GATEWAY_BASE_URL,
                Some("keep-model"),
            )
            .expect("patch pi models");
            assert_eq!(
                models["providers"]["iyw-claw"]["baseUrl"].as_str(),
                Some(MODEL_GATEWAY_BASE_URL)
            );
            assert_eq!(
                models["providers"]["iyw-claw"]["models"][0]["id"].as_str(),
                Some(MANAGED_DEFAULT_MODEL)
            );
            assert_eq!(models["custom"].as_i64(), Some(1));
        }
        _ => unreachable!(),
    }
}
