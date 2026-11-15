//! 同发布的直接安装载荷沿用 Tauri 下载、签名和持久缓存，仅部署入口改变。
use super::release::{DirectArtifact, ReleaseExtensions};
use tauri_plugin_updater::Update;

const PREFIX: &str = "https://gateway.iyw.cn/iyw-fusion-api/app-updates/artifacts/";
pub(super) fn validate(
    artifact: &DirectArtifact,
    release_id: &str,
    kind: &str,
) -> Result<(), String> {
    let expected_url = format!("{PREFIX}{}/download", artifact.artifact_id);
    if artifact.schema_version != 1
        || artifact.package_kind != kind
        || artifact.release_id != release_id
        || release_id.is_empty()
        || !release_id.bytes().all(|c| c.is_ascii_digit())
        || artifact.artifact_id.is_empty()
        || !artifact.artifact_id.bytes().all(|c| c.is_ascii_digit())
        || artifact.url != expected_url
        || artifact.target != "windows"
        || artifact.arch != "x86_64"
        || artifact.size == 0
        || artifact.size > 1024 * 1024 * 1024
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

pub(super) fn select(mut update: Update, ext: &ReleaseExtensions) -> Result<Update, String> {
    if cfg!(target_vendor = "win7") || !cfg!(target_arch = "x86_64") {
        return Ok(update);
    }
    let Some(payload) = ext.app_payload.as_ref() else {
        return Ok(update);
    };
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
