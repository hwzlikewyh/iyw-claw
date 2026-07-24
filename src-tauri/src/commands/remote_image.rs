use crate::app_error::AppCommandError;

pub async fn fetch_remote_image_core(
    url: &str,
) -> Result<crate::remote_image::RemoteImage, AppCommandError> {
    crate::remote_image::fetch(url).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn fetch_remote_image(url: String) -> Result<tauri::ipc::Response, AppCommandError> {
    let image = fetch_remote_image_core(&url).await?;
    Ok(tauri::ipc::Response::new(image.bytes))
}
