use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;

const MAX_ARGS: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CliCommand {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CliAuth {
    pub login: CliCommand,
    pub status: CliCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logout: Option<CliCommand>,
}

pub(crate) fn owner() -> Result<String, AppCommandError> {
    super::mcp::require_private_agent_storage_for_write()?;
    let root = profile()?;
    Ok(format!(
        "{:x}",
        Sha256::digest(root.to_string_lossy().as_bytes())
    ))
}

fn profile() -> Result<PathBuf, AppCommandError> {
    crate::acp::agent_storage::AgentStoragePaths::active()
        .map(|paths| paths.profile(crate::models::AgentType::Codex).root)
        .ok_or_else(|| AppCommandError::configuration_invalid("当前账号存储不可用"))
}

pub(crate) fn auth_dir(id: &str, scope: &str) -> Result<PathBuf, AppCommandError> {
    let key = format!("{:x}", Sha256::digest(id.as_bytes()));
    let space = format!("{:x}", Sha256::digest(scope.as_bytes()));
    Ok(profile()?.join("connector-auth").join(space).join(key))
}

pub(crate) async fn account_scope(
    conn: &sea_orm::DatabaseConnection,
) -> Result<String, AppCommandError> {
    super::iyw_account::iyw_account_usage_credentials_core(conn)
        .await?
        .map(|(_, scope)| scope)
        .ok_or_else(|| AppCommandError::permission_denied("请先登录爱原物账号"))
}

pub(crate) async fn require_scope(
    conn: &sea_orm::DatabaseConnection,
    spec: &Value,
) -> Result<(), AppCommandError> {
    if spec["iyw_market"]["auth_mode"].as_str() == Some("cli")
        && spec["iyw_market"]["cli_auth"]["scope"].as_str()
            != Some(account_scope(conn).await?.as_str())
    {
        return Err(AppCommandError::permission_denied(
            "此 CLI 连接器属于其他账号或企业空间，请在当前空间重新配置",
        ));
    }
    Ok(())
}

pub(crate) fn bind(raw: &Value, values: &Map<String, Value>) -> Result<Value, AppCommandError> {
    let mut resolved = raw.clone();
    super::mcp::substitute_market_parameters(&mut resolved, values, false)?;
    let plan: CliAuth = serde_json::from_value(resolved)
        .map_err(|_| AppCommandError::invalid_input("CLI 登录声明无效"))?;
    validate(&plan)?;
    Ok(json!({"plan":plan,"owner":owner()?}))
}

fn validate(plan: &CliAuth) -> Result<(), AppCommandError> {
    for command in [&plan.login, &plan.status]
        .into_iter()
        .chain(plan.logout.iter())
    {
        if command.command.trim().is_empty() || command.args.len() > MAX_ARGS {
            return Err(AppCommandError::invalid_input("CLI 程序或参数数量无效"));
        }
        if std::iter::once(&command.command)
            .chain(command.args.iter())
            .any(|arg| {
                arg.len() > MAX_ARG_BYTES || arg.contains(['\0', '\r', '\n']) || arg.contains("${")
            })
        {
            return Err(AppCommandError::invalid_input("CLI 参数包含非法内容"));
        }
    }
    Ok(())
}

pub(crate) fn plan(spec: &Value) -> Result<CliAuth, AppCommandError> {
    if spec["iyw_market"]["auth_mode"].as_str() != Some("cli") {
        return Err(AppCommandError::invalid_input("此连接器未声明 CLI 登录"));
    }
    let meta = &spec["iyw_market"]["cli_auth"];
    if meta["owner"].as_str() != Some(owner()?.as_str()) {
        return Err(AppCommandError::permission_denied(
            "连接器属于其他账号或空间，请在当前空间重新配置",
        ));
    }
    let plan: CliAuth = serde_json::from_value(meta["plan"].clone())
        .map_err(|_| AppCommandError::configuration_invalid("CLI 登录声明缺失，请重新安装"))?;
    validate(&plan)?;
    Ok(plan)
}

pub(crate) fn scope_runtime(source: &Value, resolved: &mut Value) -> Result<(), AppCommandError> {
    if source["iyw_market"]["auth_mode"].as_str() != Some("cli") {
        return Ok(());
    }
    plan(source)?;
    let id = source["iyw_market"]["id"]
        .as_str()
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器标识缺失"))?;
    let scope = source["iyw_market"]["cli_auth"]["scope"]
        .as_str()
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器账号范围缺失，请重新配置"))?;
    let home = auth_dir(id, scope)?;
    std::fs::create_dir_all(&home)
        .map_err(|_| AppCommandError::configuration_invalid("无法准备连接器账号目录"))?;
    let env = resolved
        .as_object_mut()
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器配置无效"))?
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器环境配置无效"))?;
    for (key, path) in [
        ("HOME", home.clone()),
        ("USERPROFILE", home.clone()),
        ("APPDATA", home.join("AppData/Roaming")),
        ("LOCALAPPDATA", home.join("AppData/Local")),
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("XDG_DATA_HOME", home.join(".local/share")),
        ("XDG_CACHE_HOME", home.join(".cache")),
    ] {
        std::fs::create_dir_all(&path)
            .map_err(|_| AppCommandError::configuration_invalid("无法准备连接器账号目录"))?;
        env.insert(
            key.into(),
            Value::String(path.to_string_lossy().into_owned()),
        );
    }
    Ok(())
}

pub(crate) fn environment(spec: &Value) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "TERM",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
    .collect();
    crate::acp::runtime_context::prepend_tool_dirs(
        crate::acp::agent_storage::AgentStoragePaths::active().as_ref(),
        &mut env,
    );
    if let Some(values) = spec["env"].as_object() {
        for (key, value) in values {
            if let Some(value) = value.as_str() {
                env.insert(key.clone(), value.into());
            }
        }
    }
    env
}

pub(crate) fn working_dir(spec: &Value) -> Result<String, AppCommandError> {
    spec["cwd"]
        .as_str()
        .filter(|value| !value.is_empty())
        .or_else(|| spec["env"]["HOME"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器账号目录缺失"))
}

pub(crate) async fn cleanup(
    conn: &sea_orm::DatabaseConnection,
    source: &Value,
) -> Result<(), AppCommandError> {
    if source["iyw_market"]["auth_mode"].as_str() != Some("cli") {
        return Ok(());
    }
    require_scope(conn, source).await?;
    plan(source)?;
    let id = source["iyw_market"]["id"].as_str().unwrap_or_default();
    let scope = source["iyw_market"]["cli_auth"]["scope"]
        .as_str()
        .unwrap_or_default();
    let dir = auth_dir(id, scope)?;
    if !dir.exists() {
        return Ok(());
    }
    let base = profile()?
        .join("connector-auth")
        .canonicalize()
        .map_err(|_| AppCommandError::configuration_invalid("无法核对连接器账号目录"))?;
    let target = dir
        .canonicalize()
        .map_err(|_| AppCommandError::configuration_invalid("无法核对连接器账号目录"))?;
    if !target.starts_with(&base)
        || target == base
        || dir
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(AppCommandError::configuration_invalid(
            "连接器账号目录位置异常，未执行清理",
        ));
    }
    tokio::fs::remove_dir_all(&dir).await.map_err(|_| {
        AppCommandError::configuration_invalid(
            "连接器账号目录未能清理，配置保留，请关闭相关进程后重试卸载",
        )
    })
}

pub(crate) async fn authenticated(spec: &Value) -> Result<bool, AppCommandError> {
    let plan = plan(spec)?;
    let id = spec["iyw_market"]["id"].as_str().unwrap_or_default();
    let resolved = super::mcp::hydrate_market_spec(spec)?;
    execute(&plan.status, (&resolved, id)).await
}

pub(crate) async fn execute(
    command: &CliCommand,
    context: (&Value, &str),
) -> Result<bool, AppCommandError> {
    let (spec, id) = context;
    let mut process = crate::process::tokio_command(&command.command);
    process
        .args(&command.args)
        .env_clear()
        .envs(environment(spec))
        .current_dir(working_dir(spec)?)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = process.spawn().map_err(|error| {
        tracing::warn!(connector=%id, kind=?error.kind(), "[MCP] CLI 检查程序启动失败");
        AppCommandError::task_execution_failed("CLI 程序无法启动，请检查运行时安装与发布者配置")
    })?;
    match tokio::time::timeout(CHECK_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => Ok(status.success()),
        result => {
            if let Some(pid) = child.id() {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            let _ = child.kill().await;
            tracing::warn!(connector=%id, timeout=result.is_err(), "[MCP] CLI 检查失败或超时");
            Err(AppCommandError::task_execution_failed(
                "CLI 检查失败或超时，请确认 status/logout 无需交互",
            ))
        }
    }
}
