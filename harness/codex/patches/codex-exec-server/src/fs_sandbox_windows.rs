//! 保留固定上游版本在 Windows 生产路径使用的文件助手协议。

use std::time::Duration;

use codex_exec_server_protocol::JSONRPCErrorError;
use codex_protocol::permissions::{FileSystemAccessMode, FileSystemPath, FileSystemSandboxPolicy};
use codex_utils_path_uri::{LegacyAppPathString, PathConvention, PathUri};
use codex_utils_pty::Child;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};

use super::{internal_error, invalid_request, io_error};

const FS_HELPER_EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FS_HELPER_STDERR_BYTES: u64 = 4096;

pub(super) fn bind_windows_cwd_relative_deny_read_globs(
    file_system_policy: &mut FileSystemSandboxPolicy,
    cwd: &PathUri,
) -> Result<(), JSONRPCErrorError> {
    // 助手切换到文件系统根目录前，固定相对原工作目录的拒绝规则。
    for entry in &mut file_system_policy.entries {
        if let FileSystemPath::GlobPattern { pattern } = &mut entry.path
            && entry.access == FileSystemAccessMode::Deny
            && PathConvention::Windows
                .home_relative_suffix(pattern)
                .is_none()
            && LegacyAppPathString::from_string(pattern.as_str())
                .to_path_uri(PathConvention::Windows)
                .is_err()
        {
            cwd.validate_glob_directory(PathConvention::Windows)
                .map_err(|err| invalid_request(err.to_string()))?;
            *pattern = cwd
                .join(pattern.as_str())
                .map_err(|err| invalid_request(err.to_string()))?
                .inferred_native_path_string();
        }
    }
    Ok(())
}

pub(crate) async fn read_helper_response(
    stdout: impl tokio::io::AsyncRead + Unpin,
) -> Result<Vec<u8>, JSONRPCErrorError> {
    let mut response = Vec::new();
    let bytes_read = tokio::io::BufReader::new(stdout)
        .read_until(b'\n', &mut response)
        .await
        .map_err(io_error)?;
    if bytes_read == 0 {
        return Err(internal_error(
            "fs sandbox helper closed stdout without responding".to_string(),
        ));
    }
    Ok(response)
}

pub(crate) fn drain_helper_stderr(
    child: &mut Child,
) -> tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>> {
    let stderr_pipe = child.stderr.take();
    tokio::spawn(async move {
        let mut stderr = Vec::new();
        if let Some(mut stderr_pipe) = stderr_pipe {
            (&mut stderr_pipe)
                .take(MAX_FS_HELPER_STDERR_BYTES)
                .read_to_end(&mut stderr)
                .await?;
            tokio::io::copy(&mut stderr_pipe, &mut tokio::io::sink()).await?;
        }
        Ok::<_, std::io::Error>(stderr)
    })
}

pub(crate) async fn reap_helper_after_response(
    mut child: Child,
    stderr: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>,
) -> Result<(), JSONRPCErrorError> {
    let (status, stderr) = match tokio::time::timeout(FS_HELPER_EXIT_TIMEOUT, async {
        tokio::try_join!(child.wait(), async {
            stderr.await.map_err(std::io::Error::other)?
        })
    })
    .await
    {
        Ok(result) => result.map_err(io_error)?,
        Err(_) => {
            tokio::time::timeout(FS_HELPER_EXIT_TIMEOUT, child.kill())
                .await
                .map_err(|_| {
                    internal_error("fs sandbox helper did not stop after its response".to_string())
                })?
                .map_err(io_error)?;
            return Ok(());
        }
    };
    if status.success() {
        return Ok(());
    }
    Err(internal_error(format!(
        "fs sandbox helper failed with status {status}: {stderr}",
        stderr = String::from_utf8_lossy(&stderr).trim()
    )))
}
