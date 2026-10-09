//! The pinned cua-driver release, and what the helper holds it to.
//!
//! cua-driver is downloaded into iyw-claw's binary cache, a directory the user —
//! and so any agent running as the user — can write to. On macOS the helper
//! runs it as a child that inherits the helper's TCC grants, which makes that
//! file the attack surface: replace it, and your code runs with Screen
//! Recording. So nothing about it is taken on trust from where it came from:
//!
//! * **The digests are ours.** The upstream release publishes `checksums.txt`
//!   and `release-manifest.json` next to the archives, and since 0.32.0 a
//!   Sigstore bundle for each, signed by trycua's release workflow — what
//!   upstream says, which the helper does not take at runtime. The archive
//!   and executable digests below were computed from the v0.32.0 release
//!   assets, once their bundles checked out against that workflow
//!   (`cd-rust-cua-driver.yml` at the release's tag), and are reviewed and
//!   signed with iyw-claw's own code.
//! * **On macOS the running image is checked, not the file.** The helper
//!   starts the driver suspended and checks the process that is about to run
//!   against the designated requirement, the per-architecture cdhash, the
//!   hardened-runtime flag and the entitlement list below, and only then lets
//!   it go (`helper::driver_proc`). A designated requirement alone pins the
//!   signer, not the build — every old or nightly build trycua ever signed
//!   satisfies it — which is what the cdhash is for.
//!
//! Upgrading the driver is a code change to this file: new digests, new
//! cdhashes, reviewed like any other. The managed environment prepares this
//! pinned release at startup. Settings repairs it through the environment
//! transaction; it never lets a person choose another release.

use crate::acp::error::AcpError;

/// The pinned release.
pub const DRIVER_VERSION: &str = "0.32.0";

/// The file name of the executable inside every archive, without `.exe`.
pub const DRIVER_COMMAND: &str = "cua-driver";

/// The key the driver is cached under in the binary cache: a sibling of the
/// agents' own directories, under a name no agent id can take (agent ids are
/// letters, digits, `-`, `_` and `.`), so clearing the driver's cache can
/// never clear an agent's.
pub const DRIVER_CACHE_ID: &str = "@cua-driver";

/// One platform's download, and the digests it must match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverArtifact {
    /// `registry::current_platform()` spelling.
    pub platform: &'static str,
    pub url: &'static str,
    /// Hex SHA-256 of the archive, checked before it is unpacked.
    pub archive_sha256: &'static str,
    /// Hex SHA-256 of the executable inside it, checked before every launch.
    pub executable_sha256: &'static str,
}

/// macOS ships one universal binary for both architectures: the archive
/// without `CuaDriver.app`, which iyw-claw does not use (the helper, not a
/// LaunchServices-launched app, is the TCC principal here).
pub const DRIVER_ARTIFACTS: &[DriverArtifact] = &[
    DriverArtifact {
        platform: "darwin-aarch64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-darwin-universal-binary.tar.gz",
        archive_sha256: "89b4b093db2f1c88264782f21f22f6813b7cad779c0e764ad20b5480a3f8b545",
        executable_sha256: "d5762050791535fa85a9eae2f067167287cf4a4883b2051fe2dba33204c6e8b5",
    },
    DriverArtifact {
        platform: "darwin-x86_64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-darwin-universal-binary.tar.gz",
        archive_sha256: "89b4b093db2f1c88264782f21f22f6813b7cad779c0e764ad20b5480a3f8b545",
        executable_sha256: "d5762050791535fa85a9eae2f067167287cf4a4883b2051fe2dba33204c6e8b5",
    },
    DriverArtifact {
        platform: "linux-aarch64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-linux-arm64-binary.tar.gz",
        archive_sha256: "603bf2d70e9fca06f59a74528cb1c26d47cd3eb1dceeb0827e04b6b8618e0e75",
        executable_sha256: "03167f9045602ae79b679dfc9fa749ef20bfaafcd783106a10f96aac55068057",
    },
    DriverArtifact {
        platform: "linux-x86_64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-linux-x86_64-binary.tar.gz",
        archive_sha256: "bb006010864e9a93b9f67035e345c3a076f0908d42fbe493bb21ede5f82f39a6",
        executable_sha256: "2c56cc4c260f07c957a0f8ee905c2477ec8f2a9b548024999f3698c9864c20e0",
    },
    DriverArtifact {
        platform: "windows-aarch64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-windows-arm64-binary.zip",
        archive_sha256: "b5f2c3754423f24f283d18deadf8a9fff84bf564e15babbf9690fccb815ddf2e",
        executable_sha256: "f4b192c8f13edcc594cdb6b0f535fb62dd77dfcbe99bbb3e7ee1b1f7426a3744",
    },
    DriverArtifact {
        platform: "windows-x86_64",
        url: "https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.32.0/cua-driver-rs-0.32.0-windows-x86_64-binary.zip",
        archive_sha256: "16aa3666f4ab4faba2fa6261ecb9349d2e9a0c97eca2a0b0960db5a311c2b1e1",
        executable_sha256: "d2477595e8b5ae850d119b48c8bbe5c16a1f229f3e99e706bd84622c2d7d98ae",
    },
];

/// trycua's Team ID, which signs every macOS driver build.
pub const DRIVER_TEAM_ID: &str = "YCK386LBJ7";

/// The driver's signing identifier.
pub const DRIVER_SIGNING_ID: &str = "cua-driver";

/// The designated requirement every macOS driver build satisfies: trycua's
/// Developer ID, identifier `cua-driver`. Copied from
/// `codesign -d -r- cua-driver`.
pub const DRIVER_DESIGNATED_REQUIREMENT: &str = "identifier \"cua-driver\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = YCK386LBJ7";

/// The code-directory hashes of the pinned build, one per architecture of the
/// universal binary (`codesign -dv --arch <arch>`). The running image must be
/// one of these.
pub const DRIVER_CDHASHES: &[&str] = &[
    // arm64
    "85762daaf280608d0bbd29236fe929d58342f91a",
    // x86_64
    "cca5c9457e7d3ac5bf82059b222499807fddc799",
];

/// Exactly the entitlements the pinned build carries, and must carry: Apple
/// Events (its `osascript`-backed routes) and screen capture. A build that
/// adds anything is not the build that was reviewed.
pub const DRIVER_ENTITLEMENTS: &[&str] = &[
    "com.apple.security.automation.apple-events",
    "com.apple.security.device.screen-capture",
];

/// Entitlements none of iyw-claw's executables — iyw-claw, the helper, the driver —
/// may carry. Each one lets another process of the same user put code inside
/// the one that has it: a library it did not sign, a `DYLD_*` variable, a
/// debugger. On the driver or the helper that code would run with their TCC
/// grants; on iyw-claw it would run as the process the helper trusts.
pub const DENIED_ENTITLEMENTS: &[&str] = &[
    "com.apple.security.cs.disable-library-validation",
    "com.apple.security.cs.allow-dyld-environment-variables",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.cs.disable-executable-page-protection",
    "com.apple.security.cs.debugger",
    "com.apple.security.get-task-allow",
];

/// This platform's pinned download, or `None` where cua-driver ships none.
pub fn artifact_for_current_platform() -> Option<&'static DriverArtifact> {
    let platform = crate::acp::registry::current_platform();
    DRIVER_ARTIFACTS.iter().find(|a| a.platform == platform)
}

/// Whether `entitlements` (the keys set to `true`) are exactly the pinned
/// driver's: nothing denied, nothing missing, nothing extra.
pub fn driver_entitlements_ok<S: AsRef<str>>(entitlements: &[S]) -> Result<(), String> {
    let mut have: Vec<&str> = entitlements.iter().map(AsRef::as_ref).collect();
    have.sort_unstable();
    have.dedup();
    if let Some(denied) = have.iter().find(|e| DENIED_ENTITLEMENTS.contains(e)) {
        return Err(format!("the driver carries the entitlement {denied}"));
    }
    let mut want: Vec<&str> = DRIVER_ENTITLEMENTS.to_vec();
    want.sort_unstable();
    if have != want {
        return Err(format!(
            "the driver's entitlements are {have:?}, not the pinned {want:?}"
        ));
    }
    Ok(())
}

/// Held across every change to the cached driver files — a download for a
/// helper that is starting, one a person asked for from Settings, a removal —
/// so that none of them finds the files half made by another.
static FILES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Download (or find in the cache) the pinned driver for this platform, and
/// return its path.
///
/// The archive digest is checked before anything is unpacked. The executable
/// digest is the helper's to check, before every launch — this function runs
/// in iyw-claw, and a check made here would be a check of a file an agent can
/// replace a moment later.
pub async fn ensure_driver(on_progress: impl Fn(&str)) -> Result<std::path::PathBuf, AcpError> {
    let artifact = artifact_for_current_platform().ok_or_else(|| {
        AcpError::DownloadFailed(format!(
            "cua-driver has no release for {}",
            crate::acp::registry::current_platform()
        ))
    })?;
    let _files = FILES.lock().await;
    if cfg!(feature = "tauri-runtime") {
        let path = managed_driver_path().ok_or_else(|| {
            AcpError::DownloadFailed("电脑操作组件缺失或版本不匹配，请修复应用环境后重试".into())
        })?;
        super::driver_cache::verify(&path, artifact.executable_sha256)?;
        return Ok(path);
    }
    super::driver_cache::ensure(artifact, on_progress).await
}

/// Throw every cached driver away, so the next [`ensure_driver`] downloads
/// the pinned one again: for a cached file the helper refused (a corrupted
/// download, or one someone replaced), and for a person removing the driver.
pub async fn forget_cached_driver() -> Result<(), AcpError> {
    let _files = FILES.lock().await;
    if cfg!(feature = "tauri-runtime") {
        return Err(AcpError::DownloadFailed(
            "cua-driver 由安装环境统一管理，请从环境管理中移除组件".into(),
        ));
    }
    let path = super::driver_cache::cached_path();
    if path.is_file() {
        std::fs::remove_file(path).map_err(|e| AcpError::DownloadFailed(e.to_string()))?;
    }
    Ok(())
}

/// The driver releases in the cache, newest first.
pub fn installed_driver_versions() -> Result<Vec<String>, AcpError> {
    Ok(cached_driver_path()
        .map(|_| vec![DRIVER_VERSION.to_string()])
        .unwrap_or_default())
}

/// Where the pinned release's executable is, when it is in the cache.
pub fn cached_driver_path() -> Option<std::path::PathBuf> {
    if cfg!(feature = "tauri-runtime") {
        return managed_driver_path();
    }
    let path = super::driver_cache::cached_path();
    path.is_file().then_some(path)
}

fn managed_driver_path() -> Option<std::path::PathBuf> {
    (crate::managed_environment::component_version(DRIVER_COMMAND).as_deref()
        == Some(DRIVER_VERSION))
    .then(|| crate::managed_environment::entrypoint(DRIVER_COMMAND, DRIVER_COMMAND))
    .flatten()
}
