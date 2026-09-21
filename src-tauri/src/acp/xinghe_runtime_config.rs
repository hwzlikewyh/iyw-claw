use std::collections::BTreeMap;

use crate::acp::error::AcpError;
use crate::models::AgentType;

const CONFIG_ENV: &str = "CODEX_CONFIG";
const SKILL_CATALOG_TOKEN_BUDGET: i64 = 4_096;
pub(crate) const PREFERENCES_KEY: &str = "IYW_CLAW_XINGHE_CONFIG_SNAPSHOT";
pub(crate) const AUTH_ENV: &str = "CODEX_API_KEY";

pub(crate) fn stored_preferences(
    setting: Option<&crate::db::entities::agent_setting::Model>,
) -> Option<String> {
    let environment: BTreeMap<String, String> =
        serde_json::from_str(setting?.env_json.as_deref()?).ok()?;
    environment.get(PREFERENCES_KEY).cloned()
}

pub(crate) async fn load_preferences(
    conn: &sea_orm::DatabaseConnection,
    setting: Option<&crate::db::entities::agent_setting::Model>,
    profile: &std::path::Path,
) -> Result<String, AcpError> {
    if let Some(raw) = stored_preferences(setting) {
        return Ok(raw);
    }
    // 只在首次升级时迁移已有偏好；以后启动直接读取应用设置快照。
    let raw = match std::fs::read_to_string(profile.join("config.toml")) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(AcpError::protocol("Unable to import Xinghe preferences")),
    };
    if let Some(setting) = setting {
        persist_preferences(conn, setting, &raw).await?;
    }
    Ok(raw)
}

async fn persist_preferences(
    conn: &sea_orm::DatabaseConnection,
    setting: &crate::db::entities::agent_setting::Model,
    raw: &str,
) -> Result<(), AcpError> {
    let mut environment = setting
        .env_json
        .as_deref()
        .map(serde_json::from_str::<BTreeMap<String, String>>)
        .transpose()
        .map_err(|_| AcpError::protocol("Invalid stored Xinghe environment"))?
        .unwrap_or_default();
    save_preferences(&mut environment, raw)?;
    use crate::db::entities::agent_setting::{Column, Entity};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let encoded = serde_json::to_string(&environment)
        .map_err(|_| AcpError::protocol("Unable to encode Xinghe preferences"))?;
    let result = Entity::update_many()
        .col_expr(Column::EnvJson, sea_orm::sea_query::Expr::value(encoded))
        .col_expr(
            Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .filter(Column::Id.eq(setting.id))
        .filter(Column::UpdatedAt.eq(setting.updated_at))
        .exec(conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    if result.rows_affected == 0 {
        let latest = Entity::find_by_id(setting.id)
            .one(conn)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        if stored_preferences(latest.as_ref()).as_deref() == Some(raw) {
            return Ok(());
        }
        return Err(AcpError::protocol(
            "Xinghe preferences changed during save; retry",
        ));
    }
    tracing::info!("[ACP] saved Xinghe preferences in application settings");
    Ok(())
}

pub(crate) async fn update_preferences(
    conn: &sea_orm::DatabaseConnection,
    raw: Option<&str>,
) -> Result<(), AcpError> {
    let Some(raw) = raw else {
        return Ok(());
    };
    let setting =
        crate::db::service::agent_setting_service::get_by_agent_type(conn, AgentType::Codex)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("Xinghe settings are not initialized"))?;
    persist_preferences(conn, &setting, raw).await
}

pub(crate) fn save_preferences(
    environment: &mut BTreeMap<String, String>,
    raw: &str,
) -> Result<(), AcpError> {
    toml::from_str::<toml::Table>(raw)
        .map_err(|_| AcpError::protocol("Invalid Xinghe configuration TOML"))?;
    environment.insert(PREFERENCES_KEY.into(), raw.to_string());
    Ok(())
}

/// 复用宿主受管配置投影；内核只消费启动快照，不再自行读取配置和认证文件。
pub(crate) fn project(
    environment: &mut BTreeMap<String, String>,
    native: &str,
    catalog: &std::path::Path,
) -> Result<(), AcpError> {
    let base = super::provider_overlay::model_gateway_base_url_for(AgentType::Codex);
    let raw =
        super::provider_overlay::patch_codex_toml(native, &base).map_err(AcpError::protocol)?;
    let mut config: toml::Table = toml::from_str(&raw)
        .map_err(|_| AcpError::protocol("Invalid managed Xinghe configuration"))?;
    super::codex_multi_agent::patch_toml(&mut config).map_err(AcpError::protocol)?;
    apply_skill_catalog_budget(&mut config)?;
    apply_managed_auth(&mut config)?;
    let mut values: serde_json::Map<String, serde_json::Value> = serde_json::to_value(config)
        .map_err(|_| AcpError::protocol("Invalid Xinghe configuration values"))?
        .as_object()
        .cloned()
        .ok_or_else(|| AcpError::protocol("Xinghe configuration must be an object"))?;
    values.insert(
        "model_catalog_json".into(),
        catalog.to_string_lossy().into_owned().into(),
    );
    values.insert("cli_auth_credentials_store".into(), "ephemeral".into());
    environment.insert(
        CONFIG_ENV.into(),
        serde_json::Value::Object(values).to_string(),
    );
    environment.remove("OPENAI_API_KEY");
    environment.remove(PREFERENCES_KEY);
    Ok(())
}

fn apply_skill_catalog_budget(config: &mut toml::Table) -> Result<(), AcpError> {
    let skills = config
        .entry("skills")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("Xinghe skills configuration must be a table"))?;
    // 使用原生目录裁剪，保留显式用户预算与技能全文。
    skills
        .entry("max_context_tokens")
        .or_insert(SKILL_CATALOG_TOKEN_BUDGET.into());
    Ok(())
}

fn apply_managed_auth(config: &mut toml::Table) -> Result<(), AcpError> {
    let provider = config
        .get_mut("model_providers")
        .and_then(|providers| providers.get_mut(super::provider_overlay::MANAGED_PROVIDER_ID))
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| AcpError::protocol("Managed Xinghe provider is missing"))?;
    provider.insert("env_key".into(), AUTH_ENV.into());
    provider.remove("auth");
    provider.remove("experimental_bearer_token");
    // 正式网关依赖 token 头；每次从当前进程凭据读取，避免沿用迁移快照中的旧值。
    if let Some(headers) = provider
        .get_mut("http_headers")
        .and_then(toml::Value::as_table_mut)
    {
        headers.retain(|name, _| !name.eq_ignore_ascii_case("token"));
    }
    let headers = provider
        .entry("env_http_headers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("Managed Xinghe environment headers must be a table"))?;
    headers.retain(|name, _| !name.eq_ignore_ascii_case("token"));
    headers.insert("token".into(), AUTH_ENV.into());
    Ok(())
}

pub(crate) fn apply_selected_model(
    environment: &mut BTreeMap<String, String>,
) -> Result<(), AcpError> {
    let Some(model) = environment.get("OPENAI_MODEL").cloned() else {
        return Ok(());
    };
    let mut config: serde_json::Map<String, serde_json::Value> = environment
        .get(CONFIG_ENV)
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|_| AcpError::protocol("CODEX_CONFIG must be a JSON object"))?
        .unwrap_or_default();
    config.insert("model".into(), model.into());
    environment.insert(
        CONFIG_ENV.into(),
        serde_json::Value::Object(config).to_string(),
    );
    Ok(())
}
