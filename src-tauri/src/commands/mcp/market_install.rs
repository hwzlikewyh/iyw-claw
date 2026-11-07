use super::market::request;
use super::market_parameters::{parameter_values, substitute};
use super::market_types::{Document, OptionSpec};
use super::InstallSelection;
use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;
use serde_json::{json, Map, Value};
use crate::commands::mcp_install::progress;

pub(super) async fn install(
    conn: &DatabaseConnection,
    id: &str,
    selection: &InstallSelection,
) -> Result<Value, AppCommandError> {
    let plan = progress::cancellable(request(conn, "install-plan", json!({"id":id}))).await?;
    let doc: Document = serde_json::from_value(plan["connector"].clone())
        .map_err(|_| AppCommandError::network("连接器安装计划无效"))?;
    if doc.server_id != id {
        return Err(AppCommandError::network("连接器安装计划标识不匹配"));
    }
    let option = select_option(&doc, selection)?;
    let mut values = parameter_values(option, &selection.parameter_values)?;
    if !option.artifact_id.is_empty() {
        let path = super::market_artifact::install(
            conn,
            (&doc, option, &plan["downloads"][&option.artifact_id]),
        )
        .await?;
        values.insert(
            "ARTIFACT_DIR".into(),
            Value::String(path.to_string_lossy().into_owned()),
        );
    }
    let cli_auth = bind_cli(conn, option, (&mut values, id)).await?;
    progress::begin_commit()?;
    let mut spec = bind_spec(option, values)?;
    spec["iyw_market"] = json!({"id":id,"name":doc.name,"description":doc.description,"version":doc.latest_version,"option":option.id,"agents":option.agents,"auth_mode":option.auth_mode});
    if let Some(auth) = cli_auth { spec["iyw_market"]["cli_auth"] = auth; }
    Ok(spec)
}

async fn bind_cli(conn: &DatabaseConnection, option: &OptionSpec, input: (&mut Map<String, Value>, &str)) -> Result<Option<Value>, AppCommandError> {
    if option.auth_mode != "cli" { return Ok(None); }
    let (values, id) = input;
    let raw = option.cli_auth.as_ref()
        .ok_or_else(|| AppCommandError::invalid_input("缺少 CLI 登录声明"))?;
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    values.insert("CONNECTOR_AUTH_DIR".into(), Value::String(
        crate::commands::mcp_cli_config::auth_dir(id, &scope)?.to_string_lossy().into_owned(),
    ));
    let mut public = values.clone();
    for field in option.parameters.iter().filter(|field| field.secret) { public.remove(&field.key); }
    let mut auth = crate::commands::mcp_cli_config::bind(raw, &public)?;
    auth["scope"] = Value::String(scope);
    Ok(Some(auth))
}

fn bind_spec(
    option: &OptionSpec,
    mut values: Map<String, Value>,
) -> Result<Value, AppCommandError> {
    let mut spec = option.spec.clone();
    if option.auth_mode == "cli" && spec["cwd"].as_str().is_none_or(|value| value.is_empty()) {
        spec["cwd"] = Value::String("${CONNECTOR_AUTH_DIR}".into());
    }
    let mut resolved = spec.clone();
    substitute(&mut resolved, &values, false)?;
    let resolved = super::canonicalize_spec(&resolved, "爱原物连接器市场")?;
    let has_secret = option.parameters.iter().any(|field| field.secret);
    for field in option.parameters.iter().filter(|field| field.secret) {
        values.remove(&field.key);
    }
    substitute(&mut spec, &values, true)?;
    let mut spec = super::canonicalize_spec(&spec, "爱原物连接器市场")?;
    if has_secret {
        let key = uuid::Uuid::new_v4().to_string();
        crate::keyring_store::set_connector_secret(
            &key,
            &json!({"resolved_spec":resolved}).to_string(),
        )
        .map_err(|_| AppCommandError::configuration_invalid("无法安全保存连接器凭据"))?;
        spec["iyw_secret_ref"] = Value::String(key);
    }
    Ok(spec)
}

fn select_option<'a>(
    doc: &'a Document,
    selection: &InstallSelection,
) -> Result<&'a OptionSpec, AppCommandError> {
    doc.install_options
        .iter()
        .find(|option| {
            super::market::supported_platform(option)
                && selection
                    .option_id
                    .as_ref()
                    .is_none_or(|value| value == &option.id)
                && selection
                    .protocol
                    .as_ref()
                    .is_none_or(|value| value == &option.protocol)
        })
        .ok_or_else(|| AppCommandError::invalid_input("没有适用的连接器安装选项"))
}
