use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::plugin_runtime::registry;

const OPENFILM_PLUGIN_SLUG: &str = "iyw-openfilm";
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenStudioRequest {
    pub conversation_id: Option<i64>,
    pub plugin_slug: String,
    pub expected_version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenStudioResponse {
    pub url: String,
}

pub async fn open_studio_core(
    db: &crate::db::AppDatabase,
    request: OpenStudioRequest,
) -> Result<OpenStudioResponse, AppCommandError> {
    if !cfg!(feature = "tauri-runtime") {
        return Err(AppCommandError::permission_denied(
            "OpenFilm Studio can only be opened from the desktop app",
        ));
    }
    if request.plugin_slug != OPENFILM_PLUGIN_SLUG {
        return Err(AppCommandError::invalid_input(
            "Unsupported standalone plugin",
        ));
    }
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let scope = crate::commands::mcp_cli_config::account_scope(&db.conn).await?;
    registry::reconcile_global(&db.conn).await?;
    let workspace_dir = match request.conversation_id {
        Some(conversation_id) => Some(
            super::plugin_app_context::conversation_workspace(db, conversation_id)
                .await?
                .workspace_dir,
        ),
        None => None,
    };
    let plugin = registry::global_snapshot()
        .and_then(|snapshot| snapshot.plugins.get(OPENFILM_PLUGIN_SLUG).cloned())
        .filter(|plugin| plugin.version == request.expected_version && plugin.available)
        .ok_or_else(|| AppCommandError::not_found("OpenFilm is not installed or available"))?;
    let plugin_root = std::fs::canonicalize(&plugin.install_root)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    let entrypoint = openfilm_entrypoint(&plugin_root)?;
    let node = crate::managed_environment::require_tool_entrypoint("node")
        .map_err(AppCommandError::configuration_missing)?;
    let (studio_home, shared_home) = openfilm_studio_homes(&scope, request.conversation_id)?;
    let working_dir = workspace_dir.unwrap_or_else(|| studio_home.clone());
    let output = run_openfilm(&node, &entrypoint, &working_dir, &studio_home, &shared_home).await?;
    if !output.status.success() {
        return Err(open_failed("OpenFilm Studio failed to start", &output));
    }
    let url = parse_studio_url(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| open_failed("OpenFilm did not return a valid Studio address", &output))?;
    if crate::commands::mcp_cli_config::account_scope(&db.conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "Account scope changed while OpenFilm Studio was opening",
        ));
    }
    tracing::info!(plugin = %request.plugin_slug, version = %request.expected_version, "OpenFilm Studio started");
    Ok(OpenStudioResponse { url })
}

fn openfilm_studio_homes(
    scope: &str,
    conversation_id: Option<i64>,
) -> Result<(PathBuf, PathBuf), AppCommandError> {
    use sha2::{Digest, Sha256};

    let scope_key = format!("{:x}", Sha256::digest(scope.as_bytes()));
    let base = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| AppCommandError::configuration_missing("Agent storage is unavailable"))?
        .root()
        .join("plugin-data")
        .join(OPENFILM_PLUGIN_SLUG)
        .join(scope_key)
        .join("studio");
    let shared = base.join("shared");
    let studio_home =
        base.join(conversation_id.map_or_else(|| "standalone".to_string(), |id| id.to_string()));
    std::fs::create_dir_all(&shared).map_err(AppCommandError::io)?;
    std::fs::create_dir_all(&studio_home).map_err(AppCommandError::io)?;
    Ok((
        std::fs::canonicalize(studio_home).map_err(AppCommandError::io)?,
        std::fs::canonicalize(shared).map_err(AppCommandError::io)?,
    ))
}

fn openfilm_entrypoint(root: &Path) -> Result<PathBuf, AppCommandError> {
    let entry = canonical_plugin_file(&root, "bin/openfilm.mjs")?;
    canonical_plugin_file(&root, "studio/ui/dist/index.html")?;
    Ok(entry)
}

fn canonical_plugin_file(root: &Path, relative: &str) -> Result<PathBuf, AppCommandError> {
    let file = std::fs::canonicalize(root.join(relative))
        .map_err(|_| AppCommandError::configuration_missing("OpenFilm Studio files are missing"))?;
    if !file.is_file() || !file.starts_with(root) {
        return Err(AppCommandError::permission_denied(
            "OpenFilm Studio files are outside the installed plugin",
        ));
    }
    Ok(file)
}

async fn run_openfilm(
    node: &Path,
    entrypoint: &Path,
    workspace: &Path,
    studio_home: &Path,
    shared_home: &Path,
) -> Result<std::process::Output, AppCommandError> {
    let entrypoint = node_entrypoint(entrypoint)?;
    let mut command = crate::process::tokio_command(node);
    command
        .arg(entrypoint)
        .arg("open")
        .current_dir(workspace)
        .env("OPENFILM_NO_BROWSER", "1")
        .env("OPENFILM_NO_UPDATE_CHECK", "1")
        .env("OPENFILM_HOME", studio_home)
        .env("OPENFILM_SHARED_HOME", shared_home)
        .env_remove("OPENFILM_RUN")
        .env_remove("npm_command")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    match tokio::time::timeout(OPEN_TIMEOUT, command.output()).await {
        Ok(result) => {
            result.map_err(|error| AppCommandError::configuration_invalid(error.to_string()))
        }
        Err(_) => {
            tracing::error!(
                plugin = OPENFILM_PLUGIN_SLUG,
                timeout_seconds = OPEN_TIMEOUT.as_secs(),
                "OpenFilm Studio startup timed out"
            );
            Err(AppCommandError::configuration_invalid(
                "OpenFilm Studio startup timed out",
            ))
        }
    }
}

fn node_entrypoint(entrypoint: &Path) -> Result<PathBuf, AppCommandError> {
    let path = entrypoint.to_string_lossy();
    let argument = if cfg!(windows) {
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else {
            PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path))
        }
    } else {
        entrypoint.to_path_buf()
    };
    if std::fs::canonicalize(&argument).ok().as_deref() != Some(entrypoint) {
        return Err(AppCommandError::configuration_invalid(
            "OpenFilm entrypoint cannot be represented without changing its target",
        ));
    }
    Ok(argument)
}

fn parse_studio_url(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let address = line.trim().strip_prefix("Studio")?.trim();
        let address = address.split_whitespace().next()?;
        let parsed = reqwest::Url::parse(address).ok()?;
        (parsed.scheme() == "http"
            && parsed.host_str() == Some("127.0.0.1")
            && parsed.port().is_some()
            && parsed.path() == "/open"
            && parsed
                .query_pairs()
                .any(|(key, value)| key == "key" && !value.is_empty()))
        .then(|| address.to_string())
    })
}

fn open_failed(message: &str, output: &std::process::Output) -> AppCommandError {
    let detail = safe_output_detail(output);
    tracing::error!(
        exit_code = output.status.code(),
        "OpenFilm Studio failed to start"
    );
    AppCommandError::configuration_invalid(message).with_detail(detail)
}

fn safe_output_detail(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines = stdout
        .lines()
        .chain(stderr.lines())
        .take(12)
        .map(|line| redact_local_url(line))
        .collect::<Vec<_>>()
        .join("\n");
    lines.chars().take(1000).collect()
}

fn redact_local_url(line: &str) -> String {
    let Some(start) = line.find("http://127.0.0.1:") else {
        return line.to_string();
    };
    let end = line[start..]
        .find(char::is_whitespace)
        .map(|offset| start + offset)
        .unwrap_or(line.len());
    format!("{}[Studio URL redacted]{}", &line[..start], &line[end..])
}
