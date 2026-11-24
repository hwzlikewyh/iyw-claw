use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use tokio_util::sync::CancellationToken;

use super::super::WatchError;

/// 兼容旧的一次性 HTML 调用；活动预览仍使用按需读取的分片入口。
pub(crate) async fn render(target: PathBuf) -> Result<String, WatchError> {
    let root = Arc::new(tempfile::tempdir()?);
    let cancel = CancellationToken::new();
    let guard = cancel.clone().drop_guard();
    let generation = super::import::convert(&target, root.clone(), cancel).await?;
    let html = tokio::task::spawn_blocking(move || render_sync(root.path().join(generation)))
        .await
        .map_err(|error| WatchError::Io(error.to_string()))??;
    guard.disarm();
    Ok(html)
}

fn render_sync(path: PathBuf) -> Result<String, WatchError> {
    let error = |error: hcd_core::HcdError| WatchError::StartFailed(error.to_string());
    let bundle = hcd_core::Bundle::open(path).map_err(error)?;
    let assets = read_assets(&bundle).map_err(error)?;
    let mut output = Vec::new();
    let options = hcd_core::HtmlPresentationOptions {
        text_hitboxes_enabled: false,
        image_hitboxes_enabled: false,
        override_stylesheet: Some(
            iyw_office_engine::preview_styles::read(bundle.root())
                .map_err(|error| WatchError::Io(error.to_string()))?,
        ),
        ..Default::default()
    };
    let mut writer = iyw_office_engine::word_preview::WordLayoutWriter(&mut output);
    hcd_core::render_standalone_html_with_transform(&bundle, &options, &mut writer, |html| {
        let mut result = html.to_string();
        for (hash, data) in &assets {
            result = result.replace(&format!("asset://sha256/{hash}"), data);
        }
        Ok(result)
    })
    .map_err(error)?;
    let mut html = String::from_utf8(output).map_err(|error| WatchError::Io(error.to_string()))?;
    // 字体等资源也可能出现在样式表中，不能只替换正文分片。
    for (hash, data) in assets {
        html = html.replace(&format!("asset://sha256/{hash}"), &data);
    }
    Ok(html)
}

fn read_assets(bundle: &hcd_core::Bundle) -> Result<HashMap<String, String>, hcd_core::HcdError> {
    let mut result = HashMap::new();
    let mut total = 0;
    for asset in bundle.read_asset_index()? {
        let path = bundle.resolve_href(&asset.href)?;
        if std::fs::metadata(&path)?.len() > 32 * 1024 * 1024 {
            return Err(hcd_core::HcdError::ResourceLimit(
                "Snapshot image exceeds 32 MiB".into(),
            ));
        }
        let bytes = std::fs::read(path)?;
        total += bytes.len();
        if total > 32 * 1024 * 1024 {
            return Err(hcd_core::HcdError::ResourceLimit(
                "Snapshot images exceed 32 MiB".into(),
            ));
        }
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let mime = match std::path::Path::new(&asset.href)
            .extension()
            .and_then(|value| value.to_str())
        {
            Some("svg") => "image/svg+xml",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("png") => "image/png",
            Some("woff") => "font/woff",
            Some("woff2") => "font/woff2",
            Some("ttf") => "font/ttf",
            Some("otf") => "font/otf",
            _ => "application/octet-stream",
        };
        result.insert(asset.hash, format!("data:{mime};base64,{data}"));
    }
    Ok(result)
}
