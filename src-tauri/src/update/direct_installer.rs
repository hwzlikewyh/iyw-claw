use super::{desktop::Task, direct_payload, direct_trust, release, verify};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, os::windows::fs::OpenOptionsExt, path::PathBuf, time::Duration};
use tauri::Manager;
use tauri_plugin_updater::Update;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

const MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(super) struct Installer {
    pub path: PathBuf,
    // 保持禁止写入/删除的句柄，覆盖验签、能力检查和启动交接。
    _lock: File,
}

fn valid_bytes(bytes: &[u8], metadata: &release::DirectArtifact) -> bool {
    bytes.len() as u64 == metadata.size
        && format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(&metadata.sha256)
}

fn cached(path: &std::path::Path, metadata: &release::DirectArtifact) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() != metadata.size {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    valid_bytes(&bytes, metadata).then_some(bytes)
}

async fn response(
    task: &Task,
    metadata: &release::DirectArtifact,
) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let token = crate::commands::iyw_account::iyw_account_access_token_core(&task.conn)
        .await
        .map_err(|e| e.to_string())?;
    let mut request = client.get(&metadata.url);
    if let Some(token) = token {
        request = request.header("token", token.expose());
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Installer request failed")?;
    if !response.status().is_redirection() {
        return Ok(response);
    }
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or("Missing installer object URL")?;
    let url = reqwest::Url::parse(location).map_err(|_| "Invalid installer object URL")?;
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !(host == "vol-ai.iywtu.com"
            || (host.ends_with(".volces.com") && host.contains(".tos-")))
    {
        return Err("Untrusted installer object URL".into());
    }
    // 下游鉴权只送给 Fusion，不能随重定向透传。
    client
        .get(url)
        .send()
        .await
        .map_err(|_| "Installer object download failed".into())
}

async fn download(task: &Task, metadata: &release::DirectArtifact) -> Result<Vec<u8>, String> {
    let mut response = response(task, metadata).await?;
    if !response.status().is_success() {
        return Err(format!("Installer download HTTP {}", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Installer download interrupted")?
    {
        if bytes.len() as u64 + chunk.len() as u64 > metadata.size {
            return Err("Installer download exceeds declared size".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn seal(path: PathBuf, metadata: release::DirectArtifact) -> Result<Installer, String> {
    let mut file = File::options()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != metadata.size {
        return Err("Installer size changed".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if !valid_bytes(&bytes, &metadata) {
        return Err("Installer digest changed".into());
    }
    verify::verify_release_signature(&bytes, &metadata.signature)?;
    direct_trust::verify(&path, &file)?;
    Ok(Installer { path, _lock: file })
}

fn installed_candidates(metadata: &release::DirectArtifact) -> Vec<PathBuf> {
    let Some(root) = std::env::current_exe()
        .ok()
        .and_then(|path| crate::desktop_bootstrap::resolve_install_root(&path))
    else {
        return Vec::new();
    };
    let directory = root.join("installer");
    vec![
        directory.join(format!(
            "原助理安装向导-{}.exe",
            metadata.sha256.to_ascii_lowercase()
        )),
        directory.join("原助理安装向导.exe"),
    ]
}

fn installed(metadata: &release::DirectArtifact) -> Option<Installer> {
    for path in installed_candidates(metadata) {
        if !path.is_file() {
            continue;
        }
        match seal(path, metadata.clone()) {
            Ok(installer) => {
                tracing::info!(
                    installer_sha256 = %metadata.sha256,
                    "[app-update] reusing the verified installed installer"
                );
                return Some(installer);
            }
            Err(error) => tracing::warn!(
                error = %error,
                "[app-update] installed installer is unusable; falling back to download"
            ),
        }
    }
    None
}

pub(super) async fn prepare(task: &Task, update: &Update) -> Result<Installer, String> {
    let ext = release::extensions(update).map_err(|e| e.to_string())?;
    let metadata = ext.bootstrapper.ok_or("Installer metadata is missing")?;
    direct_payload::validate(
        &metadata,
        ext.release_id.as_deref().unwrap_or(""),
        "bootstrapper",
    )?;
    if metadata.size > MAX_BYTES {
        return Err("Installer exceeds download limit".into());
    }
    let installed_metadata = metadata.clone();
    let installed = tokio::task::spawn_blocking(move || installed(&installed_metadata))
        .await
        .map_err(|error| error.to_string())?;
    if let Some(installer) = installed {
        return Ok(installer);
    }
    let directory = task
        .app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("direct-installer");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}.exe", metadata.sha256.to_ascii_lowercase()));
    if cached(&path, &metadata).is_none() {
        let bytes = download(task, &metadata).await?;
        if !valid_bytes(&bytes, &metadata) {
            return Err("Installer digest mismatch".into());
        }
        verify::verify_release_signature(&bytes, &metadata.signature)?;
        let temp = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        tokio::fs::write(temp.path(), &bytes)
            .await
            .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(&path).map_err(|e| e.to_string())?;
    }
    tokio::task::spawn_blocking(move || seal(path, metadata))
        .await
        .map_err(|e| e.to_string())?
}
