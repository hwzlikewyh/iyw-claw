use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;
use crate::terminal::manager::{SpawnOptions, TerminalManager};
use crate::web::event_bridge::EventEmitter;

const MAX_ATTEMPTS: usize = 4;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const EXIT_POLL: Duration = Duration::from_millis(500);

struct Attempt {
    owner: String,
    server: String,
    cancel: CancellationToken,
}
static ATTEMPTS: LazyLock<Mutex<BTreeMap<String, Attempt>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

struct AttemptLease(String);
impl Drop for AttemptLease {
    fn drop(&mut self) {
        release(&self.0);
    }
}

pub async fn begin(
    conn: &DatabaseConnection,
    input: (&str, &str, String),
    runtime: (&TerminalManager, EventEmitter),
) -> Result<(), AppCommandError> {
    let (id, terminal, window) = input;
    let (manager, emitter) = runtime;
    if !terminal.starts_with("mcp-cli-") || uuid::Uuid::parse_str(&terminal[8..]).is_err() {
        return Err(AppCommandError::invalid_input("CLI 终端标识无效"));
    }
    let storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let owner = super::mcp_cli_config::owner()?;
    let cancel = reserve(terminal, id, &owner)?;
    let lease = AttemptLease(terminal.to_owned());
    let options = prepare(conn, (id, terminal, window)).await?;
    if cancel.is_cancelled() {
        release(terminal);
        return Err(AppCommandError::invalid_input("登录已取消"));
    }
    let spawned = manager.spawn_with_id(options, emitter.clone());
    if spawned.is_err() {
        release(terminal);
        return Err(AppCommandError::task_execution_failed(
            "无法启动 CLI 登录，请检查程序与运行时安装",
        ));
    }
    let manager = manager.clone_ref();
    let terminal = terminal.to_owned();
    tracing::info!(connector=%id, "[MCP] CLI 交互登录已启动");
    tokio::spawn(async move {
        let _storage = storage;
        let _lease = lease;
        watch((&manager, &emitter), (&terminal, cancel)).await;
    });
    Ok(())
}

async fn prepare(
    conn: &DatabaseConnection,
    context: (&str, &str, String),
) -> Result<SpawnOptions, AppCommandError> {
    let (id, terminal, window) = context;
    let source = super::mcp_check::snapshot(conn, id).await?;
    let plan = super::mcp_cli_config::plan(&source)?;
    let spec = super::mcp::hydrate_market_spec(&source)?;
    Ok(SpawnOptions {
        terminal_id: terminal.into(),
        working_dir: super::mcp_cli_config::working_dir(&spec)?,
        owner_window_label: window,
        shell: None,
        initial_command: None,
        extra_env: Some(
            super::mcp_cli_config::environment(&spec)
                .into_iter()
                .collect(),
        ),
        temp_files: vec![],
        direct_command: Some((plan.login.command, plan.login.args)),
    })
}

async fn watch(runtime: (&TerminalManager, &EventEmitter), context: (&str, CancellationToken)) {
    let (manager, emitter) = runtime;
    let (terminal, cancel) = context;
    let finished = async {
        loop {
            tokio::time::sleep(EXIT_POLL).await;
            if !manager
                .list_with_exit_check(Some(emitter))
                .iter()
                .any(|item| item.id == terminal)
            {
                break;
            }
        }
    };
    tokio::select! { _ = cancel.cancelled() => {}, _ = tokio::time::sleep(LOGIN_TIMEOUT) => {}, _ = finished => {} }
    let _ = manager.kill(terminal);
}

fn reserve(terminal: &str, id: &str, owner: &str) -> Result<CancellationToken, AppCommandError> {
    let mut attempts = ATTEMPTS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("CLI 登录状态不可用"))?;
    if attempts.len() >= MAX_ATTEMPTS
        || attempts.contains_key(terminal)
        || attempts.values().any(|entry| entry.server == id)
    {
        return Err(AppCommandError::invalid_input(
            "请先关闭当前 CLI 登录再重试",
        ));
    }
    let cancel = CancellationToken::new();
    attempts.insert(
        terminal.into(),
        Attempt {
            owner: owner.into(),
            server: id.into(),
            cancel: cancel.clone(),
        },
    );
    Ok(cancel)
}

fn release(terminal: &str) {
    if let Ok(mut attempts) = ATTEMPTS.lock() {
        attempts.remove(terminal);
    }
}

pub(crate) fn ensure_idle(id: &str) -> Result<(), AppCommandError> {
    let attempts = ATTEMPTS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("CLI 登录状态不可用"))?;
    if attempts.values().any(|entry| entry.server == id) {
        return Err(AppCommandError::invalid_input(
            "请先关闭此连接器的 CLI 登录窗口，再修改、断开或卸载",
        ));
    }
    Ok(())
}

pub fn cancel(terminal: &str, manager: &TerminalManager) -> Result<(), AppCommandError> {
    let mut attempts = ATTEMPTS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("CLI 登录状态不可用"))?;
    if let Some(entry) = attempts.get(terminal) {
        if entry.owner != super::mcp_cli_config::owner()? {
            return Err(AppCommandError::permission_denied("CLI 登录不属于当前账号"));
        }
        entry.cancel.cancel();
        let _ = manager.kill(terminal);
        attempts.remove(terminal);
    }
    Ok(())
}

pub async fn logout(conn: &DatabaseConnection, id: &str) -> Result<(), AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let owner = super::mcp_cli_config::owner()?;
    let fingerprint = super::mcp_check::fingerprint(conn).await?;
    let operation = format!("mcp-logout-{}", uuid::Uuid::new_v4());
    reserve(&operation, id, &owner)?;
    let _lease = AttemptLease(operation);
    let source = super::mcp_check::snapshot(conn, id).await?;
    let plan = super::mcp_cli_config::plan(&source)?;
    let command = plan
        .logout
        .ok_or_else(|| AppCommandError::invalid_input("发布者未声明断开命令"))?;
    let spec = super::mcp::hydrate_market_spec(&source)?;
    if !super::mcp_cli_config::execute(&command, (&spec, id)).await? {
        return Err(AppCommandError::task_execution_failed(
            "CLI 断开未成功，请重试",
        ));
    }
    if super::mcp_cli_config::authenticated(&source).await? {
        return Err(AppCommandError::task_execution_failed(
            "CLI 仍报告已登录，请检查发布者断开命令",
        ));
    }
    super::mcp_check::require_current(conn, (id, &source, &fingerprint)).await?;
    tracing::info!(connector=%id, "[MCP] CLI 账号已断开，现有会话需重连");
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_cli_begin(
    server_id: String,
    terminal_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
    manager: tauri::State<'_, TerminalManager>,
    window: tauri::WebviewWindow,
    app_handle: tauri::AppHandle,
) -> Result<(), AppCommandError> {
    begin(
        &db.conn,
        (&server_id, &terminal_id, window.label().into()),
        (&manager, EventEmitter::Tauri(app_handle)),
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_cli_cancel(
    terminal_id: String,
    manager: tauri::State<'_, TerminalManager>,
) -> Result<(), AppCommandError> {
    cancel(&terminal_id, &manager)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_cli_logout(
    server_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<(), AppCommandError> {
    logout(&db.conn, &server_id).await
}
