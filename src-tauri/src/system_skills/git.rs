use std::path::Path;
use std::process::Output;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use semver::Version;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app_error::AppCommandError;

use super::REPOSITORY_URL;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(20);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTag {
    pub name: String,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutInfo {
    pub version: Option<String>,
    pub commit: String,
    pub dirty: bool,
}

pub fn is_newer(current: Option<&str>, latest: &Version) -> bool {
    current
        .and_then(|value| Version::parse(value.trim_start_matches('v')).ok())
        .is_none_or(|version| latest > &version)
}

pub async fn latest_stable_tag(
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<RemoteTag, AppCommandError> {
    let mut command = crate::process::tokio_command("git");
    command.args(["ls-remote", "--tags", "--refs", REPOSITORY_URL]);
    crate::git_credential::try_inject_for_url(&mut command, REPOSITORY_URL, conn, data_dir).await;
    let output = run(command, "list system skill tags", DISCOVERY_TIMEOUT).await?;
    parse_tags(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .max_by(|left, right| left.version.cmp(&right.version))
        .ok_or_else(|| AppCommandError::not_found("No stable system skill tag was found"))
}

pub async fn inspect_checkout(
    repo: &Path,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<CheckoutInfo, AppCommandError> {
    let commit = repo_output(repo, ["rev-parse", "HEAD"], conn, data_dir).await?;
    let version = repo_output(
        repo,
        ["describe", "--tags", "--exact-match"],
        conn,
        data_dir,
    )
    .await
    .ok()
    .filter(|value| Version::parse(value.trim_start_matches('v')).is_ok());
    let status = repo_output(
        repo,
        ["status", "--porcelain", "--untracked-files=no"],
        conn,
        data_dir,
    )
    .await?;
    Ok(CheckoutInfo {
        version,
        commit,
        dirty: !status.is_empty(),
    })
}

pub async fn clone_tag(
    target: &Path,
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<String, AppCommandError> {
    let mut command = crate::process::tokio_command("git");
    command
        .arg("clone")
        .args(["--depth", "1", "--branch", tag])
        .arg(REPOSITORY_URL)
        .arg(target);
    crate::git_credential::try_inject_for_url(&mut command, REPOSITORY_URL, conn, data_dir).await;
    run(command, "clone system skills", TRANSFER_TIMEOUT).await?;
    write_local_excludes(target)?;
    repo_output(target, ["rev-parse", "HEAD"], conn, data_dir).await
}

pub async fn checkout_tag(
    repo: &Path,
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<String, AppCommandError> {
    repo_output(
        repo,
        [
            "fetch",
            "--depth",
            "1",
            "origin",
            &format!("refs/tags/{tag}:refs/tags/{tag}"),
        ],
        conn,
        data_dir,
    )
    .await?;
    repo_output(
        repo,
        ["checkout", "--detach", &format!("refs/tags/{tag}")],
        conn,
        data_dir,
    )
    .await?;
    write_local_excludes(repo)?;
    repo_output(repo, ["rev-parse", "HEAD"], conn, data_dir).await
}

pub async fn checkout_commit(
    repo: &Path,
    commit: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(), AppCommandError> {
    repo_output(repo, ["checkout", "--detach", commit], conn, data_dir)
        .await
        .map(|_| ())
}

fn parse_tags(raw: &str) -> Vec<RemoteTag> {
    raw.lines()
        .filter_map(|line| {
            let (_, reference) = line.split_once(char::is_whitespace)?;
            let name = reference.trim().strip_prefix("refs/tags/")?;
            let version = Version::parse(name.strip_prefix('v')?).ok()?;
            version.pre.is_empty().then(|| RemoteTag {
                name: name.to_string(),
                version,
            })
        })
        .collect()
}

async fn repo_output<const N: usize>(
    repo: &Path,
    args: [&str; N],
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<String, AppCommandError> {
    let mut command = crate::process::tokio_command("git");
    command.arg("-C").arg(repo).args(args);
    crate::git_credential::try_inject_for_repo(
        &mut command,
        &repo.to_string_lossy(),
        conn,
        data_dir,
    )
    .await;
    let output = run(command, "update system skills", TRANSFER_TIMEOUT).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn run(
    mut command: Command,
    operation: &'static str,
    duration: Duration,
) -> Result<Output, AppCommandError> {
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .kill_on_drop(true);
    let output = timeout(duration, command.output())
        .await
        .map_err(|_| AppCommandError::external_command(operation, "Git operation timed out"))?
        .map_err(|error| AppCommandError::external_command(operation, error.to_string()))?;
    if output.status.success() {
        tracing::debug!(
            target: "system_skills",
            operation,
            status = %output.status,
            "Git operation completed"
        );
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    tracing::warn!(
        target: "system_skills",
        operation,
        status = %output.status,
        stderr,
        "Git operation failed"
    );
    Err(AppCommandError::external_command(
        operation,
        if stderr.is_empty() {
            format!("Git exited with status {}", output.status)
        } else {
            stderr
        },
    ))
}

fn write_local_excludes(repo: &Path) -> Result<(), AppCommandError> {
    let path = repo.join(".git").join("info").join("exclude");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    const RULES: &str =
        "\n# iyw-claw runtime files\n.venv/\n.venv.system-update-backup/\n__pycache__/\n*.pyc\n";
    if !existing.contains("# iyw-claw runtime files") {
        std::fs::create_dir_all(path.parent().unwrap_or(repo)).map_err(AppCommandError::io)?;
        std::fs::write(&path, format!("{existing}{RULES}")).map_err(AppCommandError::io)?;
    }
    Ok(())
}
