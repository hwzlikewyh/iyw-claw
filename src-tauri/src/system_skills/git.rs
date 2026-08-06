use std::path::Path;
use std::process::Output;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use semver::Version;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app_error::AppCommandError;

use super::{BUILTIN_PASSWORD, BUILTIN_USERNAME, REPOSITORY_URL};

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
    inject_credentials_for_url(&mut command, REPOSITORY_URL, conn, data_dir).await;
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
    inject_credentials_for_url(&mut command, REPOSITORY_URL, conn, data_dir).await;
    run(command, "clone system skills", TRANSFER_TIMEOUT).await?;
    write_local_excludes(target)?;
    repo_output(target, ["rev-parse", "HEAD"], conn, data_dir).await
}

/// Check out `tag`, overwriting local state unconditionally.
///
/// A system skill checkout is a managed mirror of the upstream repository, not a
/// working copy anyone is expected to edit. Treating local edits as a conflict to
/// resolve only ever strands the user on a stale version, so an update discards
/// them: `--force` on fetch lets a moved tag overwrite its local ref, and the
/// reset/clean pair drops tracked edits and stray untracked files before the
/// checkout. Excludes are written first so `.venv` and friends survive the clean.
pub async fn checkout_tag(
    repo: &Path,
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<String, AppCommandError> {
    require_origin_remote(repo).await?;
    write_local_excludes(repo)?;
    repo_output(
        repo,
        [
            "fetch",
            "--force",
            "--depth",
            "1",
            "origin",
            &format!("refs/tags/{tag}:refs/tags/{tag}"),
        ],
        conn,
        data_dir,
    )
    .await?;
    discard_local_changes(repo, conn, data_dir).await;
    repo_output(
        repo,
        [
            "checkout",
            "--detach",
            "--force",
            &format!("refs/tags/{tag}"),
        ],
        conn,
        data_dir,
    )
    .await?;
    write_local_excludes(repo)?;
    repo_output(repo, ["rev-parse", "HEAD"], conn, data_dir).await
}

/// Drop tracked modifications and untracked files so the next checkout cannot
/// fail on "local changes would be overwritten". Failures are logged rather than
/// propagated: the forced checkout that follows is the operation that matters,
/// and it reports its own error if the tree is still unusable.
async fn discard_local_changes(repo: &Path, conn: &DatabaseConnection, data_dir: &Path) {
    for args in [["reset", "--hard"], ["clean", "-fd"]] {
        if let Err(error) = repo_output(repo, args, conn, data_dir).await {
            tracing::warn!(
                target: "system_skills",
                operation = ?args,
                "failed to discard local system skill changes: {error}"
            );
        }
    }
}

/// Check out `commit`, overwriting local state unconditionally.
///
/// Used by rollback and by the restore path after a failed validation. Both run
/// against a tree that may carry local edits, so they need the same forced
/// semantics as [`checkout_tag`] -- otherwise a dirty mirror could not be
/// rolled back at all.
pub async fn checkout_commit(
    repo: &Path,
    commit: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(), AppCommandError> {
    write_local_excludes(repo)?;
    discard_local_changes(repo, conn, data_dir).await;
    repo_output(
        repo,
        ["checkout", "--detach", "--force", commit],
        conn,
        data_dir,
    )
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
    if !crate::git_credential::try_inject_for_repo(
        &mut command,
        &repo.to_string_lossy(),
        conn,
        data_dir,
    )
    .await
    {
        inject_builtin_credentials(&mut command, data_dir);
    }
    let output = run(command, "update system skills", TRANSFER_TIMEOUT).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Inject credentials for a network git command against the system skills
/// repository: a configured account matching the remote host when one exists,
/// otherwise the built-in account.
///
/// A configured account always wins so a deployment can rotate away from the
/// built-in credential without shipping a new build.
async fn inject_credentials_for_url(
    command: &mut Command,
    remote_url: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) {
    if crate::git_credential::try_inject_for_url(command, remote_url, conn, data_dir).await {
        return;
    }
    inject_builtin_credentials(command, data_dir);
}

/// Fall back to the credential compiled into the binary.
///
/// The askpass indirection is what makes this usable at all: the secret travels
/// to git through a child-process environment variable, so it stays out of the
/// command line (visible in the process list) and out of `.git/config`. It is
/// still recoverable from the shipped binary, which is why
/// [`BUILTIN_PASSWORD`](super::BUILTIN_PASSWORD) must stay read-only on the
/// system skills repository.
///
/// A failure here is logged rather than propagated: git then runs without
/// credentials and reports its own authentication error, which is more
/// actionable than a message about a missing askpass script.
fn inject_builtin_credentials(command: &mut Command, data_dir: &Path) {
    match crate::git_credential::ensure_askpass_script(data_dir) {
        Ok(askpass) => {
            tracing::debug!(
                target: "system_skills",
                username = BUILTIN_USERNAME,
                "no configured account matched; using built-in system skills credentials"
            );
            crate::git_credential::inject_credentials(
                command,
                BUILTIN_USERNAME,
                BUILTIN_PASSWORD,
                &askpass,
            );
        }
        Err(error) => tracing::warn!(
            target: "system_skills",
            "failed to prepare the askpass script for built-in credentials: {error}"
        ),
    }
}

/// Verify the checkout still has a readable origin remote before a fetch, so a
/// corrupted mirror fails with that cause rather than an authentication error.
async fn require_origin_remote(repo: &Path) -> Result<(), AppCommandError> {
    crate::git_credential::get_remote_url_by_name(&repo.to_string_lossy(), "origin")
        .await
        .map(|_| ())
        .ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "System skills repository has no readable origin remote",
            )
        })
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

/// Marker line introducing the managed exclude block. Unchanged from the first
/// version that wrote one, so a block an older build left behind is recognised.
const EXCLUDE_MARKER: &str = "# iyw-claw runtime files";

/// Paths that must survive the `git clean -fd` in [`checkout_tag`].
///
/// These are installed runtime environments and their build artifacts: untracked,
/// expensive to rebuild, and living inside the checkout because a skill switching
/// to a managed link migrates them there. A name missing from this list is a name
/// an update deletes.
const EXCLUDE_RULES: [&str; 5] = [
    ".venv/",
    ".venv.system-update-backup/",
    "node_modules/",
    "__pycache__/",
    "*.pyc",
];

/// Bring the managed block in `.git/info/exclude` up to the current rule set.
///
/// The block is rewritten on every call rather than written once behind a marker
/// check. The marker-check version could not add a rule after the fact: on a
/// repository an older build had already marked, the check passed and the new
/// rule never landed, leaving the path it protects exposed to `git clean -fd`.
/// That is exactly how `node_modules/` came to be missing.
///
/// Rewriting drops every line belonging to the block — the marker, and any line
/// matching a managed rule wherever it sits — then appends the block afresh.
/// Lines the user added themselves are kept, in order.
fn write_local_excludes(repo: &Path) -> Result<(), AppCommandError> {
    let path = repo.join(".git").join("info").join("exclude");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let mut kept: Vec<&str> = existing
        .lines()
        .filter(|line| {
            let line = line.trim();
            line != EXCLUDE_MARKER && !EXCLUDE_RULES.contains(&line)
        })
        .collect();
    // Otherwise the blank line that separated the previous block accumulates,
    // one per rewrite.
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }

    let mut contents = String::new();
    for line in kept {
        contents.push_str(line);
        contents.push('\n');
    }
    if !contents.is_empty() {
        contents.push('\n');
    }
    contents.push_str(EXCLUDE_MARKER);
    contents.push('\n');
    for rule in EXCLUDE_RULES {
        contents.push_str(rule);
        contents.push('\n');
    }

    std::fs::create_dir_all(path.parent().unwrap_or(repo)).map_err(AppCommandError::io)?;
    std::fs::write(&path, contents).map_err(AppCommandError::io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block an older build wrote: the current rules minus `node_modules/`.
    const LEGACY_BLOCK: &str = "\n# iyw-claw runtime files\n.venv/\n.venv.system-update-backup/\n__pycache__/\n*.pyc\n";

    fn exclude_path(repo: &Path) -> std::path::PathBuf {
        repo.join(".git").join("info").join("exclude")
    }

    fn write_exclude_file(repo: &Path, contents: &str) {
        let path = exclude_path(repo);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        std::fs::write(path, contents).expect("write");
    }

    fn read_exclude_file(repo: &Path) -> String {
        std::fs::read_to_string(exclude_path(repo)).expect("read")
    }

    #[test]
    fn write_local_excludes_creates_the_block_in_a_fresh_repository() {
        let temp = tempfile::tempdir().expect("tempdir");

        write_local_excludes(temp.path()).expect("write excludes");

        let contents = read_exclude_file(temp.path());
        assert!(contents.contains(EXCLUDE_MARKER));
        for rule in EXCLUDE_RULES {
            assert!(
                contents.lines().any(|line| line == rule),
                "{rule} should be excluded, got:\n{contents}"
            );
        }
    }

    #[test]
    fn write_local_excludes_adds_node_modules_to_a_legacy_block() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_exclude_file(temp.path(), LEGACY_BLOCK);
        assert!(
            !read_exclude_file(temp.path()).contains("node_modules/"),
            "precondition: the legacy block must lack the rule"
        );

        write_local_excludes(temp.path()).expect("write excludes");

        // The marker-check version returned early here, leaving a migrated
        // node_modules exposed to the `git clean -fd` in checkout_tag.
        assert!(
            read_exclude_file(temp.path())
                .lines()
                .any(|line| line == "node_modules/"),
            "a repository an older build marked must still gain the new rule"
        );
    }

    #[test]
    fn write_local_excludes_keeps_the_marker_appearing_once_across_rewrites() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_exclude_file(temp.path(), LEGACY_BLOCK);

        write_local_excludes(temp.path()).expect("first");
        let first = read_exclude_file(temp.path());
        write_local_excludes(temp.path()).expect("second");
        let second = read_exclude_file(temp.path());

        assert_eq!(first, second, "rewriting must be idempotent");
        assert_eq!(
            second.lines().filter(|line| *line == EXCLUDE_MARKER).count(),
            1,
            "the marker must not accumulate:\n{second}"
        );
        for rule in EXCLUDE_RULES {
            assert_eq!(
                second.lines().filter(|line| line == &rule).count(),
                1,
                "{rule} must not accumulate:\n{second}"
            );
        }
    }

    #[test]
    fn write_local_excludes_preserves_lines_the_user_added() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_exclude_file(temp.path(), "# my own notes\nscratch/\n*.log\n");

        write_local_excludes(temp.path()).expect("write excludes");

        let contents = read_exclude_file(temp.path());
        for line in ["# my own notes", "scratch/", "*.log"] {
            assert!(
                contents.lines().any(|existing| existing == line),
                "{line} should be kept, got:\n{contents}"
            );
        }
        assert!(contents.contains(EXCLUDE_MARKER));
    }
}
