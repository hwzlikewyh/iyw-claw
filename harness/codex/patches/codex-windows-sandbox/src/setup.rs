use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::c_void;
use std::os::windows::io::BorrowedHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::allow::AllowDenyPaths;
use crate::allow::compute_allow_paths_for_permissions;
use crate::deny_read_resolver::resolve_windows_deny_read_paths;
use crate::helper_materialization::bundled_executable_path_for_exe;
use crate::helper_materialization::helper_bin_dir;
use crate::identity::sandbox_setup_is_complete;
use crate::logging::current_log_file_path;
use crate::logging::log_note;
use crate::path_normalization::canonical_path_key;
use crate::path_normalization::canonicalize_path;
use crate::resolved_permissions::ResolvedWindowsSandboxPermissions;
use crate::setup_error::SetupErrorCode;
use crate::setup_error::SetupFailure;
use crate::setup_error::clear_setup_error_report;
use crate::setup_error::extract_failure;
use crate::setup_error::failure;
use crate::setup_error::read_setup_error_report;
use crate::ssh_config_dependencies::ssh_config_dependency_paths;
use anyhow::Result;
use anyhow::anyhow;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_protocol::models::PermissionProfile;
use codex_utils_absolute_path::AbsolutePathBuf;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Security::AllocateAndInitializeSid;
use windows_sys::Win32::Security::CheckTokenMembership;
use windows_sys::Win32::Security::FreeSid;
use windows_sys::Win32::Security::SECURITY_NT_AUTHORITY;

pub const SETUP_VERSION: u32 = 5;
pub const OFFLINE_USERNAME: &str = "CodexSandboxOffline";
pub const ONLINE_USERNAME: &str = "CodexSandboxOnline";
const ERROR_CANCELLED: u32 = 1223;
const SECURITY_BUILTIN_DOMAIN_RID: u32 = 0x0000_0020;
const DOMAIN_ALIAS_RID_ADMINS: u32 = 0x0000_0220;
const SETUP_EXE_FILENAME: &str = "xinghe-windows-sandbox-setup.exe";
const USERPROFILE_ROOT_EXCLUSIONS: &[&str] = &[
    ".ssh",
    ".tsh",
    ".brev",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".config",
    ".npm",
    ".pki",
    ".terraform.d",
];
const WINDOWS_PLATFORM_DEFAULT_READ_ROOTS: &[&str] = &[
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"C:\ProgramData",
];

#[derive(Clone)]
struct SharedSetupError {
    code: Option<SetupErrorCode>,
    message: String,
}

impl SharedSetupError {
    fn from_error(error: &anyhow::Error) -> Self {
        match extract_failure(error) {
            Some(failure) => Self {
                code: Some(failure.code),
                message: failure.message.clone(),
            },
            None => Self {
                code: None,
                message: format!("{error:#}"),
            },
        }
    }

    fn into_error(self) -> anyhow::Error {
        match self.code {
            Some(code) => failure(code, self.message),
            None => anyhow!(self.message),
        }
    }
}

struct SetupFlight {
    result: Mutex<Option<Result<(), SharedSetupError>>>,
    completed: Condvar,
}

impl SetupFlight {
    fn pending() -> Self {
        Self {
            result: Mutex::new(None),
            completed: Condvar::new(),
        }
    }

    fn complete(&self, result: Result<(), SharedSetupError>) {
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
        self.completed.notify_all();
    }

    fn wait(&self) -> Result<()> {
        let mut result = self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while result.is_none() {
            result = self
                .completed
                .wait(result)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        match result.clone() {
            Some(result) => result.map_err(SharedSetupError::into_error),
            None => Err(anyhow!("setup flight completed without a result")),
        }
    }
}

static SETUP_FLIGHTS: OnceLock<Mutex<HashMap<String, Arc<SetupFlight>>>> = OnceLock::new();

fn run_setup_singleflight(key: String, run: impl FnOnce() -> Result<()>) -> Result<()> {
    let flights = SETUP_FLIGHTS.get_or_init(|| Mutex::new(HashMap::new()));
    let (flight, is_leader) = {
        let mut flights = flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match flights.get(&key) {
            Some(flight) => (Arc::clone(flight), false),
            None => {
                let flight = Arc::new(SetupFlight::pending());
                flights.insert(key.clone(), Arc::clone(&flight));
                (flight, true)
            }
        }
    };

    if !is_leader {
        return flight.wait();
    }

    let result = run();
    let shared_result = match &result {
        Ok(()) => Ok(()),
        Err(error) => Err(SharedSetupError::from_error(error)),
    };
    flight.complete(shared_result);
    let mut flights = flights
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if flights
        .get(&key)
        .is_some_and(|current| Arc::ptr_eq(current, &flight))
    {
        flights.remove(&key);
    }
    result
}

pub fn sandbox_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(".sandbox")
}

pub fn sandbox_bin_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(".sandbox-bin")
}

pub fn sandbox_secrets_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(".sandbox-secrets")
}

pub fn setup_marker_path(codex_home: &Path) -> PathBuf {
    sandbox_dir(codex_home).join("setup_marker.json")
}

pub fn sandbox_users_path(codex_home: &Path) -> PathBuf {
    sandbox_secrets_dir(codex_home).join("sandbox_users.json")
}

pub struct SandboxSetupRequest<'a> {
    pub permissions: &'a ResolvedWindowsSandboxPermissions,
    pub command_cwd: &'a Path,
    pub env_map: &'a HashMap<String, String>,
    pub codex_home: &'a Path,
    pub proxy_enforced: bool,
}

#[derive(Default)]
pub struct SetupRootOverrides {
    pub read_roots: Option<Vec<PathBuf>>,
    pub read_roots_include_platform_defaults: bool,
    pub write_roots: Option<Vec<PathBuf>>,
    pub deny_read_paths: Option<Vec<PathBuf>>,
    pub deny_write_paths: Option<Vec<PathBuf>>,
}

pub fn run_setup_refresh(
    permission_profile: &PermissionProfile,
    workspace_roots: &[AbsolutePathBuf],
    command_cwd: &Path,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
    proxy_enforced: bool,
) -> Result<()> {
    let Ok(permissions) =
        ResolvedWindowsSandboxPermissions::try_from_permission_profile_for_workspace_roots(
            permission_profile,
            workspace_roots,
        )
    else {
        return Ok(());
    };
    let deny_read_paths =
        setup_refresh_deny_read_paths(permission_profile, workspace_roots, command_cwd)?;
    run_setup_refresh_inner(
        SandboxSetupRequest {
            permissions: &permissions,
            command_cwd,
            env_map,
            codex_home,
            proxy_enforced,
        },
        SetupRootOverrides {
            deny_read_paths: Some(deny_read_paths),
            ..SetupRootOverrides::default()
        },
        /*offline_proxy_settings_override*/ None,
    )
}

pub(crate) fn run_setup_refresh_with_overrides_and_proxy_settings(
    request: SandboxSetupRequest<'_>,
    overrides: SetupRootOverrides,
    offline_proxy_settings: &OfflineProxySettings,
) -> Result<()> {
    run_setup_refresh_inner(request, overrides, Some(offline_proxy_settings))
}

pub fn run_setup_refresh_with_extra_read_roots(
    permission_profile: &PermissionProfile,
    workspace_roots: &[AbsolutePathBuf],
    command_cwd: &Path,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
    extra_read_roots: Vec<PathBuf>,
    proxy_enforced: bool,
) -> Result<()> {
    let Ok(permissions) =
        ResolvedWindowsSandboxPermissions::try_from_permission_profile_for_workspace_roots(
            permission_profile,
            workspace_roots,
        )
    else {
        return Ok(());
    };
    let deny_read_paths =
        setup_refresh_deny_read_paths(permission_profile, workspace_roots, command_cwd)?;
    let mut read_roots = gather_read_roots(command_cwd, &permissions, env_map, codex_home);
    read_roots.extend(extra_read_roots);
    run_setup_refresh_inner(
        SandboxSetupRequest {
            permissions: &permissions,
            command_cwd,
            env_map,
            codex_home,
            proxy_enforced,
        },
        SetupRootOverrides {
            read_roots: Some(read_roots),
            read_roots_include_platform_defaults: false,
            write_roots: Some(Vec::new()),
            deny_read_paths: Some(deny_read_paths),
            deny_write_paths: None,
        },
        /*offline_proxy_settings_override*/ None,
    )
}

fn setup_refresh_deny_read_paths(
    permission_profile: &PermissionProfile,
    workspace_roots: &[AbsolutePathBuf],
    command_cwd: &Path,
) -> Result<Vec<PathBuf>> {
    let (mut file_system, _) = permission_profile.to_runtime_permissions();
    file_system.remove_skip_missing_path_entries();
    let file_system = file_system.materialize_project_roots_with_workspace_roots(workspace_roots);
    let command_cwd = AbsolutePathBuf::from_absolute_path(command_cwd)?;
    resolve_windows_deny_read_paths(&file_system, &command_cwd)
        .map(|paths| {
            paths
                .into_iter()
                .map(AbsolutePathBuf::into_path_buf)
                .collect()
        })
        .map_err(|err| anyhow!(err))
}

fn run_setup_refresh_inner(
    request: SandboxSetupRequest<'_>,
    overrides: SetupRootOverrides,
    offline_proxy_settings_override: Option<&OfflineProxySettings>,
) -> Result<()> {
    if !request.permissions.is_enforceable_by_windows_sandbox() {
        anyhow::bail!("unsupported filesystem permissions for Windows sandbox setup");
    }
    let (read_roots, write_roots) = build_payload_roots(&request, &overrides);
    let deny_read_paths = build_payload_deny_read_paths(overrides.deny_read_paths);
    let deny_write_paths = build_payload_deny_write_paths(&request, overrides.deny_write_paths);
    let offline_proxy_settings =
        offline_proxy_settings_for_request(&request, offline_proxy_settings_override);
    let payload = ElevationPayload {
        version: SETUP_VERSION,
        offline_username: OFFLINE_USERNAME.to_string(),
        online_username: ONLINE_USERNAME.to_string(),
        codex_home: request.codex_home.to_path_buf(),
        command_cwd: request.command_cwd.to_path_buf(),
        read_roots,
        write_roots,
        deny_read_paths,
        deny_write_paths,
        proxy_ports: offline_proxy_settings.proxy_ports,
        allow_local_binding: offline_proxy_settings.allow_local_binding,
        otel: None,
        real_user: std::env::var("USERNAME").unwrap_or_else(|_| "Administrators".to_string()),
        mode: SetupMode::Full,
        refresh_only: true,
    };
    let json = serde_json::to_vec(&payload)?;
    let b64 = BASE64_STANDARD.encode(json);
    run_setup_singleflight(b64.clone(), || {
        run_setup_refresh_payload(&b64, request.codex_home)
    })
}

fn run_setup_refresh_payload(b64: &str, codex_home: &Path) -> Result<()> {
    let exe = find_setup_exe();
    let sbx_dir = sandbox_dir(codex_home);
    let log_path = current_log_file_path(&sbx_dir);
    let cleared_report = match clear_setup_error_report(codex_home) {
        Ok(()) => true,
        Err(err) => {
            log_note(
                &format!("setup refresh: failed to clear setup_error.json before launch: {err}"),
                Some(&sbx_dir),
            );
            false
        }
    };
    // Refresh should never request elevation; ensure verb isn't set and we don't trigger UAC.
    let mut cmd = Command::new(&exe);
    cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    cmd.arg(b64)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let cwd = std::env::current_dir().unwrap_or_else(|_| codex_home.to_path_buf());
    log_note(
        &format!(
            "setup refresh: spawning {} (cwd={}, payload_len={})",
            exe.display(),
            cwd.display(),
            b64.len()
        ),
        Some(&sbx_dir),
    );
    let status = cmd.status().map_err(|err| {
        let message = format!(
            "setup refresh failed to launch helper: helper={}, cwd={}, log={}, error={err}",
            exe.display(),
            cwd.display(),
            log_path.display()
        );
        log_note(&format!("setup refresh: {message}"), Some(&sbx_dir));
        failure(SetupErrorCode::OrchestratorHelperLaunchFailed, message)
    })?;
    if !status.success() {
        log_note(
            &format!("setup refresh: exited with status {status:?}"),
            Some(&sbx_dir),
        );
        return Err(report_helper_failure(
            codex_home,
            cleared_report,
            status.code(),
        ));
    }
    if let Err(err) = clear_setup_error_report(codex_home) {
        log_note(
            &format!("setup refresh: failed to clear setup_error.json after success: {err}"),
            Some(&sbx_dir),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupMarker {
    pub version: u32,
    pub offline_username: String,
    pub online_username: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub proxy_ports: Vec<u16>,
    #[serde(default)]
    pub allow_local_binding: bool,
}

impl SetupMarker {
    pub fn version_matches(&self) -> bool {
        self.version == SETUP_VERSION
    }

    pub(crate) fn offline_proxy_settings(&self) -> OfflineProxySettings {
        OfflineProxySettings {
            proxy_ports: self.proxy_ports.clone(),
            allow_local_binding: self.allow_local_binding,
        }
    }

    pub(crate) fn request_mismatch_reason(
        &self,
        network_identity: SandboxNetworkIdentity,
        offline_proxy_settings: &OfflineProxySettings,
    ) -> Option<String> {
        if !network_identity.uses_offline_identity() {
            return None;
        }
        if self.proxy_ports == offline_proxy_settings.proxy_ports
            && self.allow_local_binding == offline_proxy_settings.allow_local_binding
        {
            return None;
        }
        Some(format!(
            "offline firewall settings changed (stored_ports={:?}, desired_ports={:?}, stored_allow_local_binding={}, desired_allow_local_binding={})",
            self.proxy_ports,
            offline_proxy_settings.proxy_ports,
            self.allow_local_binding,
            offline_proxy_settings.allow_local_binding
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxUserRecord {
    pub username: String,
    /// DPAPI-encrypted password blob, base64 encoded.
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxUsersFile {
    pub version: u32,
    pub offline: SandboxUserRecord,
    pub online: SandboxUserRecord,
}

impl SandboxUsersFile {
    pub fn version_matches(&self) -> bool {
        self.version == SETUP_VERSION
    }
}

fn is_elevated() -> Result<bool> {
    unsafe {
        let mut administrators_group: *mut c_void = std::ptr::null_mut();
        let ok = AllocateAndInitializeSid(
            &SECURITY_NT_AUTHORITY,
            2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut administrators_group,
        );
        if ok == 0 {
            return Err(anyhow!(
                "AllocateAndInitializeSid failed: {}",
                GetLastError()
            ));
        }
        let mut is_member = 0i32;
        let check = CheckTokenMembership(0, administrators_group, &mut is_member as *mut _);
        FreeSid(administrators_group as *mut _);
        if check == 0 {
            return Err(anyhow!("CheckTokenMembership failed: {}", GetLastError()));
        }
        Ok(is_member != 0)
    }
}

fn canonical_existing(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter_map(|p| {
            if !p.exists() {
                return None;
            }
            Some(dunce::canonicalize(p).unwrap_or_else(|_| p.clone()))
        })
        .collect()
}

fn profile_read_roots(user_profile: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(user_profile) {
        Ok(entries) => entries,
        Err(_) => return vec![user_profile.to_path_buf()],
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| (entry.file_name(), entry.path()))
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            !USERPROFILE_ROOT_EXCLUSIONS
                .iter()
                .any(|excluded| name.eq_ignore_ascii_case(excluded))
        })
        .map(|(_, path)| path)
        .collect()
}

fn gather_helper_read_roots(codex_home: &Path) -> Vec<PathBuf> {
    let helper_dir = helper_bin_dir(codex_home);
    let _ = std::fs::create_dir_all(&helper_dir);
    vec![helper_dir]
}

fn gather_full_read_roots_for_permissions(
    command_cwd: &Path,
    permissions: &ResolvedWindowsSandboxPermissions,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
) -> Vec<PathBuf> {
    let mut roots = gather_helper_read_roots(codex_home);
    roots.extend(
        WINDOWS_PLATFORM_DEFAULT_READ_ROOTS
            .iter()
            .map(PathBuf::from),
    );
    if let Ok(up) = std::env::var("USERPROFILE") {
        roots.extend(profile_read_roots(Path::new(&up)));
    }
    roots.push(command_cwd.to_path_buf());
    roots.extend(
        permissions
            .writable_roots_for_cwd(command_cwd, env_map)
            .into_iter()
            .map(|root| root.root),
    );
    roots.extend(
        permissions
            .readable_roots_for_cwd(command_cwd)
            .into_iter()
            .filter(|root| root.parent().is_some() || !command_cwd.starts_with(root)),
    );
    canonical_existing(&roots)
}

pub(crate) fn gather_read_roots(
    command_cwd: &Path,
    permissions: &ResolvedWindowsSandboxPermissions,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
) -> Vec<PathBuf> {
    if permissions.has_symbolic_root_read_access(command_cwd) {
        return gather_full_read_roots_for_permissions(
            command_cwd,
            permissions,
            env_map,
            codex_home,
        );
    }

    let mut roots = gather_helper_read_roots(codex_home);
    if permissions.include_platform_defaults() {
        roots.extend(
            WINDOWS_PLATFORM_DEFAULT_READ_ROOTS
                .iter()
                .map(PathBuf::from),
        );
    }
    roots.extend(permissions.readable_roots_for_cwd(command_cwd));
    canonical_existing(&roots)
}

pub(crate) fn gather_write_roots_for_permissions(
    permissions: &ResolvedWindowsSandboxPermissions,
    command_cwd: &Path,
    env_map: &HashMap<String, String>,
) -> Vec<PathBuf> {
    let roots = permissions
        .writable_roots_for_cwd(command_cwd, env_map)
        .into_iter()
        .map(|root| root.root)
        .collect::<Vec<_>>();
    let mut dedup: HashSet<PathBuf> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in canonical_existing(&roots) {
        if dedup.insert(r.clone()) {
            out.push(r);
        }
    }
    out
}

pub(crate) fn effective_write_roots_for_setup(
    permissions: &ResolvedWindowsSandboxPermissions,
    command_cwd: &Path,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
    write_roots_override: Option<&[PathBuf]>,
) -> Vec<PathBuf> {
    effective_write_roots_for_permissions(
        permissions,
        command_cwd,
        env_map,
        codex_home,
        write_roots_override,
    )
}

pub(crate) fn effective_write_roots_for_permissions(
    permissions: &ResolvedWindowsSandboxPermissions,
    command_cwd: &Path,
    env_map: &HashMap<String, String>,
    codex_home: &Path,
    write_roots_override: Option<&[PathBuf]>,
) -> Vec<PathBuf> {
    let write_roots = if let Some(roots) = write_roots_override {
        canonical_existing(roots)
    } else {
        gather_write_roots_for_permissions(permissions, command_cwd, env_map)
    };
    let write_roots = expand_user_profile_root(write_roots);
    let write_roots = filter_user_profile_root(write_roots);
    let write_roots = filter_user_profile_root_exclusions(write_roots);
    let write_roots = filter_ssh_config_dependency_roots(write_roots);
    filter_sensitive_write_roots(write_roots, codex_home)
}

#[derive(Serialize)]
struct ElevationPayload {
    version: u32,
    offline_username: String,
    online_username: String,
    codex_home: PathBuf,
    command_cwd: PathBuf,
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    #[serde(default)]
    deny_read_paths: Vec<PathBuf>,
    #[serde(default)]
    deny_write_paths: Vec<PathBuf>,
    proxy_ports: Vec<u16>,
    #[serde(default)]
    allow_local_binding: bool,
    otel: Option<codex_otel::StatsigMetricsSettings>,
    real_user: String,
    mode: SetupMode,
    #[serde(default)]
    refresh_only: bool,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SetupMode {
    Full,
    InteractiveProvision,
    ProvisionOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfflineProxySettings {
    pub proxy_ports: Vec<u16>,
    pub allow_local_binding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SandboxNetworkIdentity {
    Offline,
    Online,
}

impl SandboxNetworkIdentity {
    pub(crate) fn from_permissions(
        permissions: &ResolvedWindowsSandboxPermissions,
        proxy_enforced: bool,
    ) -> Self {
        if proxy_enforced || !permissions.network_policy().is_enabled() {
            Self::Offline
        } else {
            Self::Online
        }
    }

    pub(crate) fn uses_offline_identity(self) -> bool {
        matches!(self, Self::Offline)
    }
}

pub(crate) const PROXY_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "WS_PROXY",
    "WSS_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "ws_proxy",
    "wss_proxy",
];
const ALLOW_LOCAL_BINDING_ENV_KEY: &str = "CODEX_NETWORK_ALLOW_LOCAL_BINDING";
// Internal wire format shared with network-proxy/src/proxy.rs. The value is a comma-separated,
// sorted list of non-zero loopback proxy ports used only when computing the Windows offline
// sandbox setup marker.
const WINDOWS_SANDBOX_PROXY_PORTS_ENV_KEY: &str = "CODEX_WINDOWS_SANDBOX_PROXY_PORTS";

pub(crate) fn offline_proxy_settings_from_env(
    env_map: &HashMap<String, String>,
    network_identity: SandboxNetworkIdentity,
) -> OfflineProxySettings {
    if !network_identity.uses_offline_identity() {
        return OfflineProxySettings {
            proxy_ports: vec![],
            allow_local_binding: false,
        };
    }
    OfflineProxySettings {
        proxy_ports: proxy_ports_from_env(env_map),
        allow_local_binding: env_map
            .get(ALLOW_LOCAL_BINDING_ENV_KEY)
            .is_some_and(|value| value == "1"),
    }
}

fn offline_proxy_settings_for_request(
    request: &SandboxSetupRequest<'_>,
    offline_proxy_settings_override: Option<&OfflineProxySettings>,
) -> OfflineProxySettings {
    offline_proxy_settings_override.cloned().unwrap_or_else(|| {
        let network_identity =
            SandboxNetworkIdentity::from_permissions(request.permissions, request.proxy_enforced);
        offline_proxy_settings_from_env(request.env_map, network_identity)
    })
}

pub(crate) fn proxy_ports_from_env(env_map: &HashMap<String, String>) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for key in PROXY_ENV_KEYS {
        if let Some(value) = env_map.get(*key)
            && let Some(port) = loopback_proxy_port_from_url(value)
        {
            ports.insert(port);
        }
    }
    if let Some(value) = env_map.get(WINDOWS_SANDBOX_PROXY_PORTS_ENV_KEY) {
        ports.extend(
            value
                .split(',')
                .filter_map(|port| port.trim().parse::<u16>().ok())
                .filter(|port| *port != 0),
        );
    }
    ports.into_iter().collect()
}

pub(crate) fn loopback_proxy_port_from_url(url: &str) -> Option<u16> {
    let authority = url.trim().split_once("://")?.1.split('/').next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);

    if let Some(host) = host_port.strip_prefix('[') {
        let (host, rest) = host.split_once(']')?;
        if host != "::1" {
            return None;
        }
        let port = rest.strip_prefix(':')?.parse::<u16>().ok()?;
        return (port != 0).then_some(port);
    }

    let (host, port) = host_port.rsplit_once(':')?;
    if !(host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1") {
        return None;
    }
    let port = port.parse::<u16>().ok()?;
    (port != 0).then_some(port)
}

fn quote_arg(arg: &str) -> String {
    let needs = arg.is_empty()
        || arg
            .chars()
            .any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '"'));
    if !needs {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut bs = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => {
                bs += 1;
            }
            '"' => {
                out.push_str(&"\\".repeat(bs * 2 + 1));
                out.push('"');
                bs = 0;
            }
            _ => {
                if bs > 0 {
                    out.push_str(&"\\".repeat(bs));
                    bs = 0;
                }
                out.push(ch);
            }
        }
    }
    if bs > 0 {
        out.push_str(&"\\".repeat(bs * 2));
    }
    out.push('"');
    out
}

fn find_setup_exe() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(setup_exe) = find_setup_exe_for_current_exe(&exe)
    {
        return setup_exe;
    }
    PathBuf::from(SETUP_EXE_FILENAME)
}

fn find_setup_exe_for_current_exe(exe: &Path) -> Option<PathBuf> {
    bundled_executable_path_for_exe(exe, SETUP_EXE_FILENAME)
}

fn report_helper_failure(
    codex_home: &Path,
    cleared_report: bool,
    exit_code: Option<i32>,
) -> anyhow::Error {
    let exit_detail = format!("setup helper exited with status {exit_code:?}");
    if !cleared_report {
        return failure(SetupErrorCode::OrchestratorHelperExitNonzero, exit_detail);
    }
    match read_setup_error_report(codex_home) {
        Ok(Some(report)) => anyhow::Error::new(SetupFailure::from_report(report)),
        Ok(None) => failure(SetupErrorCode::OrchestratorHelperExitNonzero, exit_detail),
        Err(err) => failure(
            SetupErrorCode::OrchestratorHelperReportReadFailed,
            format!("{exit_detail}; failed to read setup_error.json: {err}"),
        ),
    }
}

fn verify_setup_completed(codex_home: &Path) -> Result<()> {
    if sandbox_setup_is_complete(codex_home) {
        Ok(())
    } else {
        Err(failure(
            SetupErrorCode::OrchestratorHelperIncomplete,
            "setup helper exited successfully before setup completed",
        ))
    }
}

fn run_setup_exe(
    payload: &ElevationPayload,
    needs_elevation: bool,
    codex_home: &Path,
    retained_handles: &[BorrowedHandle<'_>],
) -> Result<()> {
    let payload_json = serde_json::to_string(payload).map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorPayloadSerializeFailed,
            format!("failed to serialize elevation payload: {err}"),
        )
    })?;
    let payload_b64 = BASE64_STANDARD.encode(payload_json.as_bytes());
    if !retained_handles.is_empty() {
        // Service requests are serialized and must not join a bare setup flight
        // whose helper was started without these directory protections.
        return run_setup_exe_payload(&payload_b64, needs_elevation, codex_home, retained_handles);
    }
    run_setup_singleflight(payload_b64.clone(), || {
        run_setup_exe_payload(&payload_b64, needs_elevation, codex_home, retained_handles)
    })
}

fn run_setup_exe_payload(
    payload_b64: &str,
    needs_elevation: bool,
    codex_home: &Path,
    retained_handles: &[BorrowedHandle<'_>],
) -> Result<()> {
    use windows_sys::Win32::System::Threading::GetExitCodeProcess;
    use windows_sys::Win32::System::Threading::INFINITE;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::SEE_MASK_NOASYNC;
    use windows_sys::Win32::UI::Shell::SEE_MASK_NOCLOSEPROCESS;
    use windows_sys::Win32::UI::Shell::SHELLEXECUTEINFOW;
    use windows_sys::Win32::UI::Shell::ShellExecuteExW;
    let exe = find_setup_exe();
    let cleared_report = match clear_setup_error_report(codex_home) {
        Ok(()) => true,
        Err(err) => {
            log_note(
                &format!(
                    "setup orchestrator: failed to clear setup_error.json before launch: {err}"
                ),
                Some(&sandbox_dir(codex_home)),
            );
            false
        }
    };

    if !needs_elevation {
        let status = if retained_handles.is_empty() {
            Command::new(&exe)
                .arg(payload_b64)
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
        } else {
            crate::setup_launch::spawn_with_retained_handles(
                Command::new(&exe).arg(payload_b64),
                retained_handles,
            )
            .and_then(|mut child| child.wait())
        }
        .map_err(|err| {
            failure(
                SetupErrorCode::OrchestratorHelperLaunchFailed,
                format!("failed to launch setup helper (non-elevated): {err}"),
            )
        })?;
        if !status.success() {
            return Err(report_helper_failure(
                codex_home,
                cleared_report,
                status.code(),
            ));
        }
        verify_setup_completed(codex_home)?;
        if let Err(err) = clear_setup_error_report(codex_home) {
            log_note(
                &format!(
                    "setup orchestrator: failed to clear setup_error.json after success: {err}"
                ),
                Some(&sandbox_dir(codex_home)),
            );
        }
        return Ok(());
    }

    let exe_w = crate::winutil::to_wide(&exe);
    let params = quote_arg(payload_b64);
    let params_w = crate::winutil::to_wide(params);
    let verb_w = crate::winutil::to_wide("runas");
    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    // Sandbox setup runs on a Tokio worker without a Windows message loop.
    // ShellExecuteEx requires synchronous activation on such threads.
    sei.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    sei.lpVerb = verb_w.as_ptr();
    sei.lpFile = exe_w.as_ptr();
    sei.lpParameters = params_w.as_ptr();
    // Hide the window for the elevated helper.
    sei.nShow = 0; // SW_HIDE
    let ok = unsafe { ShellExecuteExW(&mut sei) };
    if ok == 0 || sei.hProcess == 0 {
        let last_error = unsafe { GetLastError() };
        let code = if last_error == ERROR_CANCELLED {
            SetupErrorCode::OrchestratorHelperLaunchCanceled
        } else {
            SetupErrorCode::OrchestratorHelperLaunchFailed
        };
        return Err(failure(
            code,
            format!("ShellExecuteExW failed to launch setup helper: {last_error}"),
        ));
    }
    unsafe {
        WaitForSingleObject(sei.hProcess, INFINITE);
        let mut code: u32 = 1;
        GetExitCodeProcess(sei.hProcess, &mut code);
        CloseHandle(sei.hProcess);
        if code != 0 {
            return Err(report_helper_failure(
                codex_home,
                cleared_report,
                Some(code as i32),
            ));
        }
    }
    verify_setup_completed(codex_home)?;
    if let Err(err) = clear_setup_error_report(codex_home) {
        log_note(
            &format!("setup orchestrator: failed to clear setup_error.json after success: {err}"),
            Some(&sandbox_dir(codex_home)),
        );
    }
    Ok(())
}

pub fn run_elevated_setup(request: SandboxSetupRequest<'_>) -> Result<()> {
    run_elevated_setup_inner(request, /*offline_proxy_settings_override*/ None)
}

pub(crate) fn run_elevated_setup_with_proxy_settings(
    request: SandboxSetupRequest<'_>,
    offline_proxy_settings: &OfflineProxySettings,
) -> Result<()> {
    run_elevated_setup_inner(request, Some(offline_proxy_settings))
}

fn run_elevated_setup_inner(
    request: SandboxSetupRequest<'_>,
    offline_proxy_settings_override: Option<&OfflineProxySettings>,
) -> Result<()> {
    if !request.permissions.is_enforceable_by_windows_sandbox() {
        anyhow::bail!("unsupported filesystem permissions for Windows sandbox setup");
    }
    // Ensure the shared sandbox directory exists before we send it to the elevated helper.
    let sbx_dir = sandbox_dir(request.codex_home);
    std::fs::create_dir_all(&sbx_dir).map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorSandboxDirCreateFailed,
            format!("failed to create sandbox dir {}: {err}", sbx_dir.display()),
        )
    })?;
    let payload = elevated_provisioning_payload(&request, offline_proxy_settings_override);
    let needs_elevation = !is_elevated().map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorElevationCheckFailed,
            format!("failed to determine elevation state: {err}"),
        )
    })?;
    run_setup_exe(&payload, needs_elevation, request.codex_home, &[])
}

fn elevated_provisioning_payload(
    request: &SandboxSetupRequest<'_>,
    offline_proxy_settings_override: Option<&OfflineProxySettings>,
) -> ElevationPayload {
    let offline_proxy_settings =
        offline_proxy_settings_for_request(request, offline_proxy_settings_override);
    ElevationPayload {
        version: SETUP_VERSION,
        offline_username: OFFLINE_USERNAME.to_string(),
        online_username: ONLINE_USERNAME.to_string(),
        codex_home: request.codex_home.to_path_buf(),
        command_cwd: request.codex_home.to_path_buf(),
        read_roots: Vec::new(),
        write_roots: Vec::new(),
        deny_read_paths: Vec::new(),
        deny_write_paths: Vec::new(),
        proxy_ports: offline_proxy_settings.proxy_ports,
        allow_local_binding: offline_proxy_settings.allow_local_binding,
        real_user: std::env::var("USERNAME").unwrap_or_else(|_| "Administrators".to_string()),
        otel: codex_otel::global_statsig_metrics_settings(),
        mode: SetupMode::InteractiveProvision,
        refresh_only: false,
    }
}

pub fn run_elevated_provisioning_setup(
    codex_home: &Path,
    real_user: &str,
    settings: crate::WindowsSandboxProvisioningSettings,
) -> Result<()> {
    run_elevated_provisioning_setup_with_retained_handles(codex_home, real_user, settings, &[])
}

/// Runs service provisioning with directory protections retained by the helper
/// itself, so they survive an unexpected exit of the provisioning service.
pub fn run_elevated_provisioning_setup_with_retained_handles(
    codex_home: &Path,
    real_user: &str,
    settings: crate::WindowsSandboxProvisioningSettings,
    retained_handles: &[BorrowedHandle<'_>],
) -> Result<()> {
    if !codex_home.is_absolute()
        || !matches!(
            codex_home.components().next(),
            Some(std::path::Component::Prefix(prefix))
                if matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                )
        )
    {
        return Err(failure(
            SetupErrorCode::OrchestratorSandboxDirCreateFailed,
            format!(
                "sandbox provisioning CODEX_HOME must be an absolute local disk path: {}",
                codex_home.display()
            ),
        ));
    }
    let sbx_dir = sandbox_dir(codex_home);
    std::fs::create_dir_all(&sbx_dir).map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorSandboxDirCreateFailed,
            format!("failed to create sandbox dir {}: {err}", sbx_dir.display()),
        )
    })?;
    if !is_elevated().map_err(|err| {
        failure(
            SetupErrorCode::OrchestratorElevationCheckFailed,
            format!("failed to determine elevation state: {err}"),
        )
    })? {
        return Err(failure(
            SetupErrorCode::OrchestratorElevationRequired,
            "sandbox provisioning setup must be run from an elevated process",
        ));
    }
    let payload = ElevationPayload {
        version: SETUP_VERSION,
        offline_username: OFFLINE_USERNAME.to_string(),
        online_username: ONLINE_USERNAME.to_string(),
        codex_home: codex_home.to_path_buf(),
        command_cwd: codex_home.to_path_buf(),
        read_roots: Vec::new(),
        write_roots: Vec::new(),
        deny_read_paths: Vec::new(),
        deny_write_paths: Vec::new(),
        proxy_ports: settings.proxy_ports,
        allow_local_binding: settings.allow_local_binding,
        otel: codex_otel::global_statsig_metrics_settings(),
        real_user: real_user.to_string(),
        mode: SetupMode::ProvisionOnly,
        refresh_only: false,
    };
    run_setup_exe(
        &payload,
        /*needs_elevation*/ false,
        codex_home,
        retained_handles,
    )
}

pub(crate) fn build_payload_roots(
    request: &SandboxSetupRequest<'_>,
    overrides: &SetupRootOverrides,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let write_roots = effective_write_roots_for_setup(
        request.permissions,
        request.command_cwd,
        request.env_map,
        request.codex_home,
        overrides.write_roots.as_deref(),
    );
    let mut read_roots = if let Some(roots) = overrides.read_roots.as_deref() {
        // An explicit override is the split policy's complete readable set. Keep only the
        // helper/platform roots the elevated setup needs; do not re-add legacy cwd/full-read roots.
        let mut read_roots = gather_helper_read_roots(request.codex_home);
        if overrides.read_roots_include_platform_defaults {
            read_roots.extend(
                WINDOWS_PLATFORM_DEFAULT_READ_ROOTS
                    .iter()
                    .map(PathBuf::from),
            );
        }
        read_roots.extend(roots.iter().cloned());
        canonical_existing(&read_roots)
    } else {
        gather_read_roots(
            request.command_cwd,
            request.permissions,
            request.env_map,
            request.codex_home,
        )
    };
    read_roots = expand_user_profile_root(read_roots);
    read_roots = filter_user_profile_root(read_roots);
    read_roots = filter_user_profile_root_exclusions(read_roots);
    read_roots = filter_ssh_config_dependency_roots(read_roots);
    let write_root_set: HashSet<PathBuf> = write_roots.iter().cloned().collect();
    let deny_read_keys: Vec<String> = overrides
        .deny_read_paths
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|path| canonical_path_key(path))
        .collect();
    read_roots.retain(|root| {
        if write_root_set.contains(root) {
            return false;
        }
        if deny_read_keys.is_empty() {
            return true;
        }
        let root_key = canonical_path_key(root);
        !deny_read_keys
            .iter()
            .any(|denied| Path::new(&root_key).starts_with(denied))
    });
    (read_roots, write_roots)
}

pub(crate) fn build_payload_deny_write_paths(
    request: &SandboxSetupRequest<'_>,
    explicit_deny_write_paths: Option<Vec<PathBuf>>,
) -> Vec<PathBuf> {
    let allow_deny_paths: AllowDenyPaths = compute_allow_paths_for_permissions(
        request.permissions,
        request.command_cwd,
        request.env_map,
    );
    let mut deny_write_paths: Vec<PathBuf> = explicit_deny_write_paths
        .unwrap_or_default()
        .into_iter()
        .map(|path| canonicalize_path(&path))
        .collect();
    deny_write_paths.extend(allow_deny_paths.deny);
    deny_write_paths
}

fn build_payload_deny_read_paths(explicit_deny_read_paths: Option<Vec<PathBuf>>) -> Vec<PathBuf> {
    // Keep the configured spelling here so the ACL layer can plan both the
    // lexical path and any existing canonical target for reparse-point aliases.
    explicit_deny_read_paths.unwrap_or_default()
}

fn expand_user_profile_root(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let Ok(user_profile) = std::env::var("USERPROFILE") else {
        return roots;
    };
    expand_user_profile_root_for(roots, Path::new(&user_profile))
}

fn expand_user_profile_root_for(roots: Vec<PathBuf>, user_profile: &Path) -> Vec<PathBuf> {
    let user_profile_key = canonical_path_key(user_profile);
    let mut expanded = Vec::new();
    for root in roots {
        if canonical_path_key(&root) == user_profile_key {
            expanded.extend(profile_read_roots(user_profile));
        } else {
            expanded.push(root);
        }
    }

    expanded.sort_by_key(|root| canonical_path_key(root));
    expanded.dedup_by(|a, b| canonical_path_key(a.as_path()) == canonical_path_key(b.as_path()));
    expanded
}

fn filter_user_profile_root(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let Ok(user_profile) = std::env::var("USERPROFILE") else {
        return roots;
    };
    let user_profile_key = canonical_path_key(Path::new(&user_profile));
    roots.retain(|root| canonical_path_key(root) != user_profile_key);
    roots
}

fn filter_user_profile_root_exclusions(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let Ok(user_profile) = std::env::var("USERPROFILE") else {
        return roots;
    };
    let user_profile = Path::new(&user_profile);
    roots.retain(|root| !is_user_profile_root_exclusion(root, user_profile));
    roots
}

fn is_user_profile_root_exclusion(root: &Path, user_profile: &Path) -> bool {
    let root_key = canonical_path_key(root);
    let profile_key = canonical_path_key(user_profile);
    let profile_prefix = format!("{}/", profile_key.trim_end_matches('/'));
    let Some(relative_key) = root_key.strip_prefix(&profile_prefix) else {
        return false;
    };
    let Some(child_name) = relative_key
        .split('/')
        .next()
        .filter(|name| !name.is_empty())
    else {
        return false;
    };

    USERPROFILE_ROOT_EXCLUSIONS
        .iter()
        .any(|excluded| child_name.eq_ignore_ascii_case(excluded))
}

fn filter_ssh_config_dependency_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let Ok(user_profile) = std::env::var("USERPROFILE") else {
        return roots;
    };
    let user_profile = Path::new(&user_profile);
    let dependency_paths = ssh_config_dependency_paths(user_profile);
    roots.retain(|root| !is_ssh_config_dependency_root(root, user_profile, &dependency_paths));
    roots
}

fn is_ssh_config_dependency_root(
    root: &Path,
    user_profile: &Path,
    dependency_paths: &[PathBuf],
) -> bool {
    let Some(child_name) = user_profile_child_name(root, user_profile) else {
        return false;
    };

    dependency_paths.iter().any(|path| {
        user_profile_child_name(path, user_profile)
            .is_some_and(|dependency_child| child_name.eq_ignore_ascii_case(&dependency_child))
    })
}

fn user_profile_child_name(path: &Path, user_profile: &Path) -> Option<String> {
    let root_key = canonical_path_key(path);
    let profile_key = canonical_path_key(user_profile);
    let profile_prefix = format!("{}/", profile_key.trim_end_matches('/'));
    let relative_key = root_key.strip_prefix(&profile_prefix)?;
    relative_key
        .split('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn filter_sensitive_write_roots(mut roots: Vec<PathBuf>, codex_home: &Path) -> Vec<PathBuf> {
    // Never grant capability write access to CODEX_HOME or anything under CODEX_HOME/.sandbox,
    // CODEX_HOME/.sandbox-bin, or CODEX_HOME/.sandbox-secrets. These locations contain sandbox
    // control/state and helper binaries and must remain tamper-resistant.
    let codex_home_key = canonical_path_key(codex_home);
    let sbx_dir_key = canonical_path_key(&sandbox_dir(codex_home));
    let sbx_dir_prefix = format!("{}/", sbx_dir_key.trim_end_matches('/'));
    let sbx_bin_dir_key = canonical_path_key(&sandbox_bin_dir(codex_home));
    let sbx_bin_dir_prefix = format!("{}/", sbx_bin_dir_key.trim_end_matches('/'));
    let secrets_dir_key = canonical_path_key(&sandbox_secrets_dir(codex_home));
    let secrets_dir_prefix = format!("{}/", secrets_dir_key.trim_end_matches('/'));

    roots.retain(|root| {
        let key = canonical_path_key(root);
        key != codex_home_key
            && key != sbx_dir_key
            && !key.starts_with(&sbx_dir_prefix)
            && key != sbx_bin_dir_key
            && !key.starts_with(&sbx_bin_dir_prefix)
            && key != secrets_dir_key
            && !key.starts_with(&secrets_dir_prefix)
    });
    roots
}
