use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::models::agent::AgentType;

const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmmirror.com";
const NPM_REGISTRY_ENV: &str = "IYW_CLAW_NPM_REGISTRY";

pub fn private_npm_prefix(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Result<PathBuf, AcpError> {
    let version = version
        .trim()
        .strip_prefix(['v', 'V'])
        .unwrap_or(version.trim())
        .trim();
    if version.is_empty()
        || !version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'))
        || matches!(version, "." | "..")
    {
        return Err(AcpError::DownloadFailed(
            "npm runtime version is invalid".to_string(),
        ));
    }
    Ok(paths
        .npm_runtime_dir()
        .join(registry::registry_id_for(agent_type))
        .join(version)
        .join(registry::current_platform()))
}

pub fn npm_prefix_bin_dir(prefix: &Path) -> PathBuf {
    if cfg!(windows) {
        prefix.to_path_buf()
    } else {
        prefix.join("bin")
    }
}

pub fn npm_registry(explicit: Option<&str>) -> Result<String, AcpError> {
    let value = explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_NPM_REGISTRY);
    let parsed = reqwest::Url::parse(value)
        .map_err(|error| AcpError::DownloadFailed(format!("invalid npm registry URL: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AcpError::DownloadFailed(
            "npm registry must be an absolute HTTP(S) URL".to_string(),
        ));
    }
    Ok(value.to_string())
}

/// The registry every managed `npm install` must go through: the
/// `IYW_CLAW_NPM_REGISTRY` override when set, otherwise the npmmirror default
/// (mainland-China acceleration). Callers that build an npm command by hand
/// should use [`npm_registry_arg`] rather than re-deriving this, so no install
/// path silently falls back to registry.npmjs.org.
pub fn configured_npm_registry() -> Result<String, AcpError> {
    match std::env::var(NPM_REGISTRY_ENV) {
        Ok(value) => npm_registry(Some(&value)),
        Err(std::env::VarError::NotPresent) => npm_registry(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(AcpError::DownloadFailed(
            "npm registry environment is not valid Unicode".to_string(),
        )),
    }
}

/// `--registry=<configured>`, ready to push onto any npm invocation.
pub fn npm_registry_arg() -> Result<OsString, AcpError> {
    Ok(OsString::from(format!(
        "--registry={}",
        configured_npm_registry()?
    )))
}

pub fn private_npm_install_args(
    prefix: &Path,
    cache: &Path,
    packages: &[&str],
) -> Result<Vec<OsString>, AcpError> {
    private_npm_install_args_with_registry(prefix, cache, packages, None)
}

pub fn private_npm_install_args_with_registry(
    prefix: &Path,
    cache: &Path,
    packages: &[&str],
    registry: Option<&str>,
) -> Result<Vec<OsString>, AcpError> {
    let registry = match registry {
        Some(registry) => npm_registry(Some(registry))?,
        None => configured_npm_registry()?,
    };
    let mut args = vec![
        OsString::from("install"),
        OsString::from("--global"),
        OsString::from("--json"),
        OsString::from("--include=optional"),
        OsString::from("--no-audit"),
        OsString::from("--no-fund"),
        OsString::from("--prefer-offline"),
        OsString::from(format!("--registry={registry}")),
        path_arg("--prefix=", prefix),
        path_arg("--cache=", cache),
    ];
    args.extend(packages.iter().map(OsString::from));
    Ok(args)
}

pub(crate) fn path_arg(name: &str, path: &Path) -> OsString {
    let mut value = OsString::from(name);
    value.push(path.as_os_str());
    value
}

/// The npm platform token for the host, in the `<os>-<arch>` form npm packages
/// conventionally use for per-platform binary sub-packages (esbuild, sharp,
/// `@openai/codex`, …). `None` on a target whose npm spelling we don't know, in
/// which case the optional-dependency audit below is skipped rather than
/// guessed at.
pub fn host_npm_platform_token() -> Option<String> {
    let os = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

/// Names of `optionalDependencies` in `manifest` that target the host platform.
///
/// npm treats a failed optional dependency as a warning and still exits 0, so a
/// per-platform binary package that failed to download leaves an install that
/// looks successful and only breaks when the agent is launched. These are the
/// entries whose absence is therefore fatal rather than optional. Matching is
/// on the *key* (the alias, e.g. `@openai/codex-win32-x64`) containing the host
/// token, which holds regardless of whether the value is a version range or an
/// `npm:` alias.
fn host_platform_optional_deps(manifest: &serde_json::Value, host_token: &str) -> Vec<String> {
    manifest
        .get("optionalDependencies")
        .and_then(serde_json::Value::as_object)
        .map(|deps| {
            deps.keys()
                .filter(|name| name.contains(host_token))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a package directory under `<prefix>` the way Node would: the
/// top-level `node_modules`, or nested inside another package.
fn npm_package_dir(prefix: &Path, package: &str) -> PathBuf {
    let mut dir = prefix.join("node_modules");
    for segment in package.split('/') {
        dir = dir.join(segment);
    }
    dir
}

/// Whether `package` is resolvable from `start` under Node's algorithm: every
/// `node_modules` on the walk up from the declaring package to `prefix`.
///
/// Checking only "nested inside the declaring package" and "hoisted to the
/// prefix" is not enough. npm hoists a transitive platform package to the
/// *nearest shared* `node_modules`, which for a bundled agent is an
/// intermediate level: `@openai/codex` declares `@openai/codex-win32-x64`, but
/// npm places it at
/// `<prefix>/node_modules/@agentclientprotocol/codex-acp/node_modules/@openai/codex-win32-x64`
/// — a sibling of the declaring package, neither nested under it nor at the
/// prefix root. Node finds it there; a two-location check does not, and the
/// audit then rejects a perfectly good install on every machine.
fn npm_dependency_resolves(prefix: &Path, start: &Path, package: &str) -> bool {
    let mut current = Some(start);
    while let Some(dir) = current {
        if npm_package_dir(dir, package).join("package.json").is_file() {
            return true;
        }
        if dir == prefix {
            break;
        }
        current = dir.parent();
    }
    // `start` is normally under `prefix`; check the prefix regardless so an
    // unexpected layout cannot turn into a false negative.
    npm_package_dir(prefix, package)
        .join("package.json")
        .is_file()
}

/// Every `package.json` under `<prefix>/node_modules` that declares
/// `optionalDependencies`, walking nested `node_modules` because npm nests a
/// dependency's own platform packages (codex-acp bundles `@openai/codex`, whose
/// binary sub-packages live under *its* `node_modules`).
fn manifests_with_optional_deps(root: &Path, depth: usize, out: &mut Vec<(PathBuf, PathBuf)>) {
    // 6 levels of nesting is far beyond what npm's hoisting produces in
    // practice; the bound just keeps a symlink cycle from hanging the install.
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Scoped packages (`@scope/name`) are a directory level, not a package.
        if name.starts_with('@') {
            manifests_with_optional_deps(&path, depth + 1, out);
            continue;
        }
        let manifest = path.join("package.json");
        if manifest.is_file() {
            out.push((path.clone(), manifest));
        }
        let nested = path.join("node_modules");
        if nested.is_dir() {
            manifests_with_optional_deps(&nested, depth + 1, out);
        }
    }
}

/// Fail when a host-platform optional dependency declared by any installed
/// package did not actually land on disk.
///
/// This is the check that catches npm's silent optional-dependency failure. It
/// is deliberately generic (driven by `optionalDependencies` + the host token)
/// so every npm agent is covered without a per-agent list — the packages are
/// large (`@openai/codex` unpacks to ~390 MB) and mainland-China networks drop
/// them often enough that "npm exited 0" is not evidence of a usable install.
pub fn verify_host_platform_optional_deps(prefix: &Path) -> Result<(), AcpError> {
    let Some(host_token) = host_npm_platform_token() else {
        return Ok(());
    };
    let mut manifests = Vec::new();
    manifests_with_optional_deps(&prefix.join("node_modules"), 0, &mut manifests);

    for (package_dir, manifest_path) in manifests {
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        for dependency in host_platform_optional_deps(&manifest, &host_token) {
            // npm may satisfy it nested under the declaring package, hoisted to
            // the install prefix, or at any intermediate `node_modules` in
            // between; Node's resolution accepts all of them.
            if npm_dependency_resolves(prefix, package_dir.as_path(), &dependency) {
                continue;
            }
            return Err(AcpError::DownloadFailed(format!(
                "npm skipped the platform binary '{dependency}' required on this machine \
                 ({host_token}); npm reports optional-dependency failures as warnings and \
                 still exits successfully, so the download most likely failed or ran out of \
                 disk space. Retry the installation."
            )));
        }
    }
    Ok(())
}

pub fn resolve_private_npm_command(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    command: &str,
) -> Option<PathBuf> {
    let prefix = private_npm_prefix(paths, agent_type, version).ok()?;
    resolve_npm_command_from_prefix(&prefix, command)
}

pub fn preferred_private_npm_command_path(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    command: &str,
) -> Result<PathBuf, AcpError> {
    let prefix = private_npm_prefix(paths, agent_type, version)?;
    let bin_dir = npm_prefix_bin_dir(&prefix);
    if cfg!(windows) {
        Ok(bin_dir.join(format!("{command}.cmd")))
    } else {
        Ok(bin_dir.join(command))
    }
}

pub fn private_npm_staging_prefix(paths: &AgentStoragePaths, agent_type: AgentType) -> PathBuf {
    paths.staging_dir().join(format!(
        "npm-{}-{}",
        registry::registry_id_for(agent_type),
        uuid::Uuid::new_v4()
    ))
}

fn resolve_npm_command_from_prefix(prefix: &Path, command: &str) -> Option<PathBuf> {
    let bin_dir = npm_prefix_bin_dir(prefix);
    let local_bin = prefix.join("node_modules").join(".bin");

    #[cfg(windows)]
    let candidates = [
        bin_dir.join(format!("{command}.cmd")),
        bin_dir.join(format!("{command}.exe")),
        bin_dir.join(command),
        local_bin.join(format!("{command}.cmd")),
        local_bin.join(format!("{command}.exe")),
        local_bin.join(command),
    ];
    #[cfg(not(windows))]
    let candidates = [bin_dir.join(command), local_bin.join(command)];

    candidates
        .into_iter()
        .find(|candidate| is_command_candidate(candidate))
}

pub fn activate_private_npm_runtime(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    staging_prefix: &Path,
    required_commands: &[&str],
) -> Result<PathBuf, AcpError> {
    for command in required_commands {
        if resolve_npm_command_from_prefix(staging_prefix, command).is_none() {
            let _ = std::fs::remove_dir_all(staging_prefix);
            return Err(AcpError::DownloadFailed(format!(
                "private npm install did not produce command '{command}'"
            )));
        }
    }

    let final_prefix = match private_npm_prefix(paths, agent_type, version) {
        Ok(prefix) => prefix,
        Err(error) => {
            let _ = std::fs::remove_dir_all(staging_prefix);
            return Err(error);
        }
    };
    if let Err(error) = activate_staged_prefix(paths, staging_prefix, &final_prefix, agent_type) {
        let _ = std::fs::remove_dir_all(staging_prefix);
        return Err(error);
    }
    Ok(final_prefix)
}

fn activate_staged_prefix(
    paths: &AgentStoragePaths,
    staging_prefix: &Path,
    final_prefix: &Path,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let parent = final_prefix
        .parent()
        .ok_or_else(|| AcpError::DownloadFailed("private npm destination has no parent".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AcpError::DownloadFailed(format!("create npm runtime dir failed: {e}")))?;

    let previous = move_existing_to_trash(paths, final_prefix, agent_type)?;
    if let Err(error) = std::fs::rename(staging_prefix, final_prefix) {
        if let Some(previous) = previous.as_ref() {
            let _ = std::fs::rename(previous, final_prefix);
        }
        return Err(AcpError::DownloadFailed(format!(
            "activate private npm runtime failed: {error}"
        )));
    }
    if let Some(previous) = previous {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(())
}

fn move_existing_to_trash(
    paths: &AgentStoragePaths,
    existing: &Path,
    agent_type: AgentType,
) -> Result<Option<PathBuf>, AcpError> {
    if !existing.exists() {
        return Ok(None);
    }
    let trash = paths.trash_dir().join("npm").join(format!(
        "{}-{}",
        registry::registry_id_for(agent_type),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(trash.parent().unwrap())
        .map_err(|e| AcpError::DownloadFailed(format!("create npm trash dir failed: {e}")))?;
    std::fs::rename(existing, &trash)
        .map_err(|e| AcpError::DownloadFailed(format!("move npm runtime aside failed: {e}")))?;
    Ok(Some(trash))
}

pub fn uninstall_private_npm_runtime(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let agent_dir = paths
        .npm_runtime_dir()
        .join(registry::registry_id_for(agent_type));
    if !agent_dir.exists() || std::fs::remove_dir_all(&agent_dir).is_ok() {
        return Ok(());
    }
    let aside = move_existing_to_trash(paths, &agent_dir, agent_type)?.ok_or_else(|| {
        AcpError::DownloadFailed("private npm runtime disappeared during uninstall".into())
    })?;
    let _ = std::fs::remove_dir_all(aside);
    Ok(())
}

pub fn sweep_trash(paths: &AgentStoragePaths) {
    let Ok(entries) = std::fs::read_dir(paths.trash_dir().join("npm")) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

#[cfg(windows)]
fn is_command_candidate(path: &Path) -> bool {
    path.is_file()
}

#[cfg(not(windows))]
fn is_command_candidate(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create `<parent>/node_modules/<package>/package.json` and return the
    /// package directory, mirroring how npm lays a package down.
    fn place_package(parent: &Path, package: &str, manifest: &str) -> PathBuf {
        let dir = npm_package_dir(parent, package);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), manifest).unwrap();
        dir
    }

    /// A manifest declaring the host platform binary as an optional dependency,
    /// the way `@openai/codex` does.
    fn declares_platform_binary(host_token: &str) -> String {
        format!(
            r#"{{"name":"@openai/codex","version":"0.144.6",
                 "optionalDependencies":{{"@openai/codex-{host_token}":"0.144.6"}}}}"#
        )
    }

    const PLATFORM_MANIFEST: &str = r#"{"name":"platform-binary","version":"0.144.6"}"#;
    const ACP_MANIFEST: &str = r#"{"name":"@agentclientprotocol/codex-acp","version":"1.1.5"}"#;

    /// The layout `npm install --global --prefix=<staging>
    /// @agentclientprotocol/codex-acp` actually produces on Windows: the
    /// platform binary is hoisted to codex-acp's `node_modules`, making it a
    /// *sibling* of the `@openai/codex` that declares it — neither nested
    /// inside the declaring package nor at the prefix root.
    ///
    /// Regression test for the audit rejecting every successful install, which
    /// surfaced in the UI as 内核准备失败 on every machine.
    #[test]
    fn accepts_platform_binary_hoisted_to_an_intermediate_node_modules() {
        let Some(token) = host_npm_platform_token() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path();

        let acp = place_package(prefix, "@agentclientprotocol/codex-acp", ACP_MANIFEST);
        place_package(&acp, "@openai/codex", &declares_platform_binary(&token));
        place_package(&acp, &format!("@openai/codex-{token}"), PLATFORM_MANIFEST);

        verify_host_platform_optional_deps(prefix).unwrap();
    }

    #[test]
    fn accepts_platform_binary_nested_under_the_declaring_package() {
        let Some(token) = host_npm_platform_token() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path();

        let acp = place_package(prefix, "@agentclientprotocol/codex-acp", ACP_MANIFEST);
        let codex = place_package(&acp, "@openai/codex", &declares_platform_binary(&token));
        place_package(&codex, &format!("@openai/codex-{token}"), PLATFORM_MANIFEST);

        verify_host_platform_optional_deps(prefix).unwrap();
    }

    #[test]
    fn accepts_platform_binary_hoisted_to_the_prefix_root() {
        let Some(token) = host_npm_platform_token() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path();

        let acp = place_package(prefix, "@agentclientprotocol/codex-acp", ACP_MANIFEST);
        place_package(&acp, "@openai/codex", &declares_platform_binary(&token));
        place_package(prefix, &format!("@openai/codex-{token}"), PLATFORM_MANIFEST);

        verify_host_platform_optional_deps(prefix).unwrap();
    }

    /// The failure the audit exists to catch: npm exited 0 but skipped the
    /// platform binary, so it is on none of the resolution paths.
    #[test]
    fn rejects_a_genuinely_missing_platform_binary() {
        let Some(token) = host_npm_platform_token() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path();

        let acp = place_package(prefix, "@agentclientprotocol/codex-acp", ACP_MANIFEST);
        place_package(&acp, "@openai/codex", &declares_platform_binary(&token));

        let error = verify_host_platform_optional_deps(prefix).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("@openai/codex-{token}")),
            "error should name the missing binary, got: {error}"
        );
    }

    /// A package whose optional dependencies target *other* platforms must not
    /// be audited against this host.
    #[test]
    fn ignores_optional_dependencies_for_other_platforms() {
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path();

        place_package(
            prefix,
            "@openai/codex",
            r#"{"name":"@openai/codex","version":"0.144.6",
                "optionalDependencies":{"@openai/codex-sunos-sparc":"0.144.6"}}"#,
        );

        verify_host_platform_optional_deps(prefix).unwrap();
    }
}
