//! 同发布的直接安装载荷沿用 Tauri 下载、签名和持久缓存，仅部署入口改变。
use super::release::{DirectArtifact, ReleaseExtensions};
use tauri_plugin_updater::Update;

const PREFIX: &str = "https://gateway.iyw.cn/iyw-fusion-api/app-updates/artifacts/";

pub(super) fn platform() -> (&'static str, &'static str) {
    let target = if cfg!(target_vendor = "win7") {
        "windows7"
    } else {
        "windows"
    };
    let arch = if cfg!(target_arch = "x86") {
        "i686"
    } else {
        std::env::consts::ARCH
    };
    (target, arch)
}

pub(super) fn validate(
    artifact: &DirectArtifact,
    release_id: &str,
    kind: &str,
) -> Result<(), String> {
    let expected_url = format!("{PREFIX}{}/download", artifact.artifact_id);
    let (target, arch) = platform();
    if artifact.schema_version != 1
        || artifact.package_kind != kind
        || artifact.release_id != release_id
        || release_id.is_empty()
        || !release_id.bytes().all(|c| c.is_ascii_digit())
        || artifact.artifact_id.is_empty()
        || !artifact.artifact_id.bytes().all(|c| c.is_ascii_digit())
        || artifact.url != expected_url
        || artifact.target != target
        || artifact.arch != arch
        || artifact.size == 0
        || artifact.size > super::download_policy::HARD_MAX_BYTES
        || artifact.signature.is_empty()
        || artifact.sha256.len() != 64
        || !artifact.sha256.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Direct installer release metadata is invalid or mismatched".into());
    }
    Ok(())
}

pub(super) fn selected(update: &Update) -> bool {
    update.raw_json["direct_payload_selected"] == true
}

pub(super) fn uses_installer(update: &Update) -> bool {
    selected(update) || update.raw_json["unified_installer_selected"] == true
}

fn select_nsis(mut update: Update, ext: &ReleaseExtensions) -> Result<Update, String> {
    if let Some(installer) = ext.bootstrapper.as_ref() {
        let release_id = ext
            .release_id
            .as_deref()
            .ok_or("Installer update has no release ID")?;
        if ext.channel.map(|c| c.as_str()) != Some("stable") {
            return Err("Installer update requires stable environment bindings".into());
        }
        validate(installer, release_id, "bootstrapper")?;
        update.raw_json["unified_installer_selected"] = true.into();
        tracing::info!(version=%update.version, release_id, "[app-update] selected unified installer for signed NSIS compatibility package");
    }
    Ok(update)
}

pub(super) fn select(mut update: Update, ext: &ReleaseExtensions) -> Result<Update, String> {
    if !cfg!(any(target_arch = "x86_64", target_arch = "x86")) {
        return Ok(update);
    }
    let Some(payload) = ext.app_payload.as_ref() else {
        return select_nsis(update, ext);
    };
    if platform() != ("windows", "x86_64") {
        return Err("Direct payload is not supported on this Windows platform".into());
    }
    let release_id = ext
        .release_id
        .as_deref()
        .ok_or("Direct update has no release ID")?;
    if ext.channel.map(|c| c.as_str()) != Some("stable") {
        return Err("Direct payload requires stable environment bindings".into());
    }
    validate(payload, release_id, "app_payload_zip")?;
    validate(
        ext.bootstrapper
            .as_ref()
            .ok_or("Direct update has no signed installer")?,
        release_id,
        "bootstrapper",
    )?;
    update.download_url = payload.url.parse().map_err(|_| "Invalid payload URL")?;
    update.signature = payload.signature.clone();
    update.raw_json["size"] = payload.size.into();
    update.raw_json["sha256"] = payload.sha256.clone().into();
    update.raw_json["direct_payload_selected"] = true.into();
    tracing::info!(version=%update.version, release_id, "[app-update] selected signed app payload; NSIS remains for legacy clients");
    Ok(update)
}
