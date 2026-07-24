mod image_format;
mod network;

use tokio::sync::Semaphore;

use crate::app_error::AppCommandError;

pub const MAX_REMOTE_IMAGE_BYTES: usize = 20 * 1024 * 1024;
static REMOTE_IMAGE_FETCHES: Semaphore = Semaphore::const_new(4);

pub struct RemoteImage {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
}

pub async fn fetch(url: &str) -> Result<RemoteImage, AppCommandError> {
    let parsed_url = reqwest::Url::parse(url).ok();
    let source_scheme = parsed_url
        .as_ref()
        .map(reqwest::Url::scheme)
        .unwrap_or("invalid");
    let source_host = parsed_url
        .as_ref()
        .and_then(reqwest::Url::host_str)
        .unwrap_or("");
    let permit = REMOTE_IMAGE_FETCHES
        .acquire()
        .await
        .map_err(|_| AppCommandError::network("Remote image loader is unavailable"))?;
    let result = network::download(url, MAX_REMOTE_IMAGE_BYTES).await;
    drop(permit);
    let downloaded = match result {
        Ok(downloaded) => downloaded,
        Err(error) => {
            tracing::warn!(
                target: "remote_image",
                scheme = source_scheme,
                host = source_host,
                error_code = ?error.code,
                error_message = %error.message,
                error_detail = ?error.detail,
                "remote image download failed"
            );
            return Err(error);
        }
    };
    let info = match image_format::inspect(&downloaded.bytes) {
        Ok(info) => info,
        Err(reason) => {
            tracing::warn!(
                target: "remote_image",
                scheme = downloaded.final_url.scheme(),
                host = downloaded.final_url.host_str().unwrap_or(""),
                redirects = downloaded.redirects,
                bytes = downloaded.bytes.len(),
                reason,
                "remote image validation failed"
            );
            return Err(AppCommandError::invalid_input(reason));
        }
    };
    tracing::info!(
        target: "remote_image",
        scheme = downloaded.final_url.scheme(),
        host = downloaded.final_url.host_str().unwrap_or(""),
        redirects = downloaded.redirects,
        bytes = downloaded.bytes.len(),
        width = info.width,
        height = info.height,
        mime_type = info.mime_type,
        "remote image loaded"
    );
    Ok(RemoteImage {
        bytes: downloaded.bytes,
        mime_type: info.mime_type,
    })
}
