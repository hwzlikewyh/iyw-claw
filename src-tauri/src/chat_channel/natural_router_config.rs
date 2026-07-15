use std::time::Duration;

use reqwest::Url;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::provider_overlay::{
    model_gateway_base_url_for, MANAGED_DEFAULT_MODEL, MANAGED_MODEL_IDS,
};
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::models::agent::AgentType;

const ENABLED_KEY: &str = "chat_natural_router_enabled";
const MODEL_KEY: &str = "chat_natural_router_model";

const DEFAULT_ENABLED: bool = true;
const DEFAULT_TIMEOUT_MS: u64 = 6000;
const DEFAULT_MIN_CONFIDENCE: f32 = 0.72;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatNaturalRouterConfig {
    pub enabled: bool,
    pub api_url: String,
    pub model: String,
    pub timeout_ms: u64,
    pub min_confidence: f32,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatNaturalRouterConfigInput {
    pub enabled: bool,
    pub api_url: String,
    pub model: String,
    pub timeout_ms: u64,
    pub min_confidence: f32,
}

#[derive(Debug, Clone)]
pub struct ChatNaturalRouterRuntimeConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
    pub min_confidence: f32,
}

pub async fn get_chat_natural_router_config(
    db: &DatabaseConnection,
) -> Result<ChatNaturalRouterConfig, AppCommandError> {
    let enabled = metadata_bool(db, ENABLED_KEY, DEFAULT_ENABLED).await?;
    let api_url = normalize_chat_completions_url(&model_gateway_base_url_for(AgentType::Codex))?;
    let stored_model = metadata_string(db, MODEL_KEY, MANAGED_DEFAULT_MODEL).await?;
    let model = if MANAGED_MODEL_IDS.contains(&stored_model.as_str()) {
        stored_model
    } else {
        MANAGED_DEFAULT_MODEL.to_string()
    };
    let has_api_key = crate::commands::iyw_account::iyw_account_access_token_core(db)
        .await?
        .is_some();

    Ok(ChatNaturalRouterConfig {
        enabled,
        api_url,
        model,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        min_confidence: DEFAULT_MIN_CONFIDENCE,
        has_api_key,
    })
}

pub async fn set_chat_natural_router_config(
    db: &DatabaseConnection,
    input: ChatNaturalRouterConfigInput,
) -> Result<(), AppCommandError> {
    let model = input.model.trim();
    if !MANAGED_MODEL_IDS.contains(&model) {
        return Err(AppCommandError::invalid_input(
            "Router model must be one of the managed Agent models",
        ));
    }

    app_metadata_service::upsert_value(db, ENABLED_KEY, bool_string(input.enabled))
        .await
        .map_err(AppCommandError::from)?;
    app_metadata_service::upsert_value(db, MODEL_KEY, model)
        .await
        .map_err(AppCommandError::from)?;

    Ok(())
}

pub fn save_chat_natural_router_api_key(token: &str) -> Result<(), AppCommandError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppCommandError::invalid_input("Router API key is empty"));
    }
    crate::keyring_store::set_chat_router_token(token)
        .map_err(|e| AppCommandError::io_error("Failed to save router API key").with_detail(e))
}

pub fn delete_chat_natural_router_api_key() -> Result<(), AppCommandError> {
    crate::keyring_store::delete_chat_router_token()
        .map_err(|e| AppCommandError::io_error("Failed to delete router API key").with_detail(e))
}

pub async fn get_runtime_config(
    db: &DatabaseConnection,
) -> Result<Option<ChatNaturalRouterRuntimeConfig>, AppCommandError> {
    let config = get_chat_natural_router_config(db).await?;
    if !config.enabled {
        return Ok(None);
    }

    let api_key = crate::commands::iyw_account::iyw_account_access_token_core(db)
        .await?
        .map(|token| token.expose().to_string());
    let Some(api_key) = api_key else {
        tracing::warn!("[ChatChannel] natural router enabled but API key is missing");
        return Ok(None);
    };

    Ok(Some(ChatNaturalRouterRuntimeConfig {
        api_url: config.api_url,
        api_key,
        model: config.model,
        timeout: Duration::from_millis(config.timeout_ms),
        min_confidence: config.min_confidence,
    }))
}

fn bool_string(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

async fn metadata_string(
    db: &DatabaseConnection,
    key: &str,
    default: &str,
) -> Result<String, AppCommandError> {
    Ok(app_metadata_service::get_value(db, key)
        .await
        .map_err(AppCommandError::from)?
        .unwrap_or_else(|| default.to_string()))
}

async fn metadata_bool(
    db: &DatabaseConnection,
    key: &str,
    default: bool,
) -> Result<bool, AppCommandError> {
    let Some(value) = app_metadata_service::get_value(db, key)
        .await
        .map_err(AppCommandError::from)?
    else {
        return Ok(default);
    };
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1"
    ))
}

fn normalize_chat_completions_url(raw: &str) -> Result<String, AppCommandError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppCommandError::invalid_input("Router API URL is empty"));
    }

    let parsed = Url::parse(trimmed).map_err(|e| {
        AppCommandError::invalid_input("Router API URL is invalid").with_detail(e.to_string())
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppCommandError::invalid_input(
            "Router API URL must use http or https",
        ));
    }

    if parsed.query().is_some() || parsed.path().ends_with("/chat/completions") {
        return Ok(parsed.to_string().trim_end_matches('/').to_string());
    }

    if parsed.path().trim_end_matches('/').ends_with("/v1") {
        Ok(format!("{trimmed}/chat/completions"))
    } else {
        Ok(format!("{trimmed}/v1/chat/completions"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::fresh_in_memory_db;

    #[test]
    fn normalizes_openai_compatible_base_url() {
        assert_eq!(
            normalize_chat_completions_url("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("https://gateway.example/api").unwrap(),
            "https://gateway.example/api/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("https://openrouter.ai/api/v1/chat/completions")
                .unwrap(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn rejects_non_http_router_url() {
        assert!(normalize_chat_completions_url("file:///tmp/x").is_err());
    }

    #[tokio::test]
    async fn chat_natural_router_config_defaults_and_roundtrip() {
        let db = fresh_in_memory_db().await;
        let default = get_chat_natural_router_config(&db.conn)
            .await
            .expect("get default");
        assert!(default.enabled);
        assert_eq!(
            default.api_url,
            normalize_chat_completions_url(&model_gateway_base_url_for(AgentType::Codex)).unwrap()
        );
        assert_eq!(default.model, MANAGED_DEFAULT_MODEL);
        assert_eq!(default.timeout_ms, DEFAULT_TIMEOUT_MS);

        set_chat_natural_router_config(
            &db.conn,
            ChatNaturalRouterConfigInput {
                enabled: true,
                api_url: "https://openrouter.ai/api/v1".to_string(),
                model: MANAGED_MODEL_IDS[1].to_string(),
                timeout_ms: 3000,
                min_confidence: 0.8,
            },
        )
        .await
        .expect("set");

        let stored = get_chat_natural_router_config(&db.conn)
            .await
            .expect("get stored");
        assert!(stored.enabled);
        assert_eq!(
            stored.api_url,
            normalize_chat_completions_url(&model_gateway_base_url_for(AgentType::Codex)).unwrap()
        );
        assert_eq!(stored.model, MANAGED_MODEL_IDS[1]);
        assert_eq!(stored.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(stored.min_confidence, DEFAULT_MIN_CONFIDENCE);
    }
}
