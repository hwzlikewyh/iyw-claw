use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use super::{ChannelCaller, ChannelToolService};
use crate::chat_channel::task_context::TaskContext;
use crate::chat_channel::types::ChannelType;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const OUTPUT_LIMIT: u64 = 1024 * 1024;
const DOMAINS: &[&str] = &[
    "doc",
    "document",
    "sheet",
    "smartsheet",
    "smart-sheet",
    "smartdoc",
    "calendar",
    "contact",
    "email",
    "meeting",
    "disk",
    "todo",
];

impl ChannelToolService {
    pub(super) async fn wecom_business(
        &self,
        caller: &ChannelCaller,
        task: &TaskContext,
        input: Value,
    ) -> Result<Value, String> {
        if task.origin.channel_type != ChannelType::WecomAiBot {
            return Err("CHANNEL_BUSINESS_UNAVAILABLE".into());
        }
        let command = validate_command(&input)?;
        let request_id = input["request_id"].as_str().ok_or("REQUEST_ID_REQUIRED")?;
        let mutation = self
            .begin_mutation(caller, "wecom_business", request_id, input.clone())
            .await?;
        let model = match mutation {
            super::service::MutationStart::Return(value) => return Ok(value),
            super::service::MutationStart::Started(model) => model,
        };
        let outcome = self
            .run_business(task, &command, &input)
            .await
            .unwrap_or_else(|error| json!({"error":error,"execution_status":"unknown"}));
        self.finish_mutation(
            caller,
            "wecom_business",
            request_id,
            model,
            outcome,
            Some(task.origin.route.channel_id),
        )
        .await
    }

    async fn run_business(
        &self,
        task: &TaskContext,
        command: &[String],
        input: &Value,
    ) -> Result<Value, String> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(tokio::sync::Mutex::default).lock().await;
        crate::chat_channel::task_progress::validate_execution(task).await?;
        let (data_dir, config, bot_id, secret) = self.prepare_business(task).await?;
        crate::chat_channel::task_progress::validate_execution(task).await?;
        if self.bot_identity(task).await? != (bot_id, secret.clone()) {
            return Err("CHANNEL_CREDENTIAL_CHANGED".into());
        }
        let args = business_arguments(command, input);
        let (success, stdout) = run(&data_dir, &config, &args).await?;
        let stdout = stdout.replace(&secret, "[redacted]");
        let data =
            serde_json::from_str::<Value>(&stdout).unwrap_or_else(|_| json!({"text":stdout}));
        tracing::info!(channel_id = task.origin.route.channel_id, domain = %command[0], success,
            "[channel-business] command completed");
        let mut result = json!({"status":if success {"completed"} else {"failed"},"data":data});
        if !success {
            result["error"] = json!("BUSINESS_COMMAND_FAILED");
        }
        Ok(result)
    }

    async fn prepare_business(
        &self,
        task: &TaskContext,
    ) -> Result<(PathBuf, PathBuf, String, String), String> {
        let data_dir = self
            .manager
            .data_dir()
            .await
            .ok_or("CHANNEL_RUNTIME_UNAVAILABLE")?;
        crate::wecom_ai::ensure_cli(&data_dir)
            .await
            .map_err(|_| "MANAGED_WECOM_CLI_UNAVAILABLE")?;
        let (bot_id, secret) = self.bot_identity(task).await?;
        let fingerprint = format!("{:x}", Sha256::digest(format!("{bot_id}:{secret}")));
        let config = data_dir
            .join("wecom-ai")
            .join("channel-business")
            .join(format!(
                "{}-{}",
                task.origin.route.channel_id,
                &fingerprint[..16]
            ));
        tokio::fs::create_dir_all(&config)
            .await
            .map_err(|_| "BUSINESS_STORAGE_UNAVAILABLE")?;
        ensure_authorized(&data_dir, &config, (&bot_id, &secret)).await?;
        Ok((data_dir, config, bot_id, secret))
    }

    async fn bot_identity(&self, task: &TaskContext) -> Result<(String, String), String> {
        let channel = crate::db::service::chat_channel_service::get_by_id(
            &self.db.conn,
            task.origin.route.channel_id,
        )
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED")?
        .ok_or("CHANNEL_NOT_FOUND")?;
        if !channel.enabled || channel.channel_type != "wecom_ai_bot" {
            return Err("CHANNEL_DISABLED".into());
        }
        let config: Value =
            serde_json::from_str(&channel.config_json).map_err(|_| "CHANNEL_CONFIG_INVALID")?;
        let bot_id = config["bot_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("BOT_ID_REQUIRED")?;
        let secret = crate::keyring_store::get_channel_token(channel.id)
            .ok_or("BUSINESS_AUTHORIZATION_REQUIRED")?;
        Ok((bot_id.to_string(), secret))
    }
}

fn validate_command(input: &Value) -> Result<Vec<String>, String> {
    let args = input["command"]
        .as_array()
        .ok_or("INVALID_BUSINESS_COMMAND")?;
    let command = args
        .iter()
        .map(|arg| {
            arg.as_str()
                .map(str::to_string)
                .ok_or("INVALID_BUSINESS_COMMAND".into())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if command.is_empty()
        || command.len() > 6
        || !DOMAINS.contains(&command[0].as_str())
        || command.iter().any(|arg| {
            arg.is_empty()
                || arg.starts_with('-')
                || arg.len() > 64
                || !arg
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return Err("INVALID_BUSINESS_COMMAND".into());
    }
    Ok(command)
}

async fn run(data_dir: &Path, config: &Path, args: &[String]) -> Result<(bool, String), String> {
    let mut command = crate::wecom_ai::business_command::command(data_dir)
        .map_err(|_| "MANAGED_WECOM_CLI_UNAVAILABLE")?;
    command
        .args(args)
        .env(crate::wecom_ai::CONFIG_DIR_ENV, config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "BUSINESS_PROCESS_FAILED")?;
    let stdout = child.stdout.take().ok_or("BUSINESS_PROCESS_FAILED")?;
    let stderr = child.stderr.take().ok_or("BUSINESS_PROCESS_FAILED")?;
    let output = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdout = stdout.take(OUTPUT_LIMIT + 1);
        let mut stderr = stderr.take(OUTPUT_LIMIT + 1);
        tokio::try_join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err))?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status.success(), out, err))
    };
    let (success, out, err) = tokio::time::timeout(COMMAND_TIMEOUT, output)
        .await
        .map_err(|_| "BUSINESS_EXECUTION_UNKNOWN")?
        .map_err(|_| "BUSINESS_PROCESS_FAILED")?;
    if out.len() as u64 > OUTPUT_LIMIT || err.len() as u64 > OUTPUT_LIMIT {
        return Err("BUSINESS_OUTPUT_TOO_LARGE".into());
    }
    let output = if !success && out.is_empty() {
        &err
    } else {
        &out
    };
    Ok((success, String::from_utf8_lossy(output).into_owned()))
}

async fn ensure_authorized(
    data_dir: &Path,
    config: &Path,
    identity: (&str, &str),
) -> Result<(), String> {
    let (bot_id, secret) = identity;
    if !config.join("credentials.enc").is_file() || !config.join(".encryption_key").is_file() {
        let args = vec![
            "auth".into(),
            "init".into(),
            "--bot-id".into(),
            bot_id.to_string(),
            "--secret".into(),
            secret.to_string(),
        ];
        let auth = run(data_dir, config, &args).await?;
        if !auth.0
            || !config.join("credentials.enc").is_file()
            || !config.join(".encryption_key").is_file()
        {
            return Err("BUSINESS_AUTHORIZATION_FAILED".into());
        }
    }
    Ok(())
}

fn business_arguments(command: &[String], input: &Value) -> Vec<String> {
    let mut args = command.to_vec();
    if input["help"].as_bool() == Some(true) {
        args.push("--help".into());
    } else if input["schema"].as_bool() == Some(true) {
        args.push("--schema".into());
    } else {
        args.push("--json".into());
        args.push(
            input
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string(),
        );
    }
    args
}
