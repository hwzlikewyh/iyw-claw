//! 旧受管工具迁入用户共享目录，并保留原路径链接以兼容现有启动器。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::app_error::AppCommandError;

pub async fn prepare_shared_runtime(data_dir: &Path) -> Result<(), AppCommandError> {
    if !crate::shared_runtime::root().is_absolute() {
        return Err(AppCommandError::configuration_invalid(
            "Shared runtime requires an absolute user home directory",
        ));
    }
    let _guard = super::state::acquire_writer_lock(data_dir)
        .await?
        .ok_or_else(|| {
            AppCommandError::task_execution_failed("Shared runtime is being updated; retry shortly")
        })?;
    let mut sources = vec![data_dir.to_path_buf()];
    if let Some(paths) = crate::acp::agent_storage::AgentStoragePaths::active() {
        sources.push(paths.root().clone());
    }
    sources.dedup();
    tokio::task::spawn_blocking(move || prepare_roots(&sources))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?
        .map_err(AppCommandError::io)
}

fn prepare_roots(sources: &[PathBuf]) -> io::Result<()> {
    for source in sources {
        for tool in crate::shared_runtime::SHARED_TOOLS {
            if let Err(error) = migrate_tool(source, tool) {
                tracing::error!(tool, source = %source.display(),
                    destination = %crate::shared_runtime::tool_root(source, tool).display(),
                    error = %error, "[shared-runtime] migration failed; retained recoverable state");
                return Err(error);
            }
        }
    }
    for path in crate::shared_runtime::environment().values() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn migrate_tool(data_dir: &Path, tool: &str) -> io::Result<()> {
    let source = data_dir.join("runtime").join(tool);
    let target = crate::shared_runtime::tool_root(data_dir, tool);
    if source == target {
        return Ok(());
    }
    if super::runtime_migration_transaction::recover(&source, &target)? {
        return Ok(());
    }
    if !source.join("current.json").is_file() {
        return Ok(());
    }
    if same_directory(&source, &target) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&source)?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(io::Error::other(format!(
            "Legacy runtime is not a regular directory: {}",
            source.display()
        )));
    }
    if target.join("current.json").exists() {
        tracing::warn!(tool, source = %source.display(), target = %target.display(),
            "[shared-runtime] preserving a separate legacy runtime; shared runtime already exists");
        return Ok(());
    }
    if preserve_environment_roots(&source, &target, tool)? {
        fs::create_dir_all(&target)?;
        return migrate_versions(&source, &target, tool);
    }
    if target.exists() {
        fs::remove_dir(&target)?;
    }
    tracing::info!(tool, source = %source.display(), target = %target.display(),
        "[shared-runtime] migrating legacy tool");
    relocate(&source, &target)?;
    tracing::info!(tool, source = %source.display(), target = %target.display(),
        "[shared-runtime] migrated tool and retained compatibility link");
    Ok(())
}

fn preserve_environment_roots(source: &Path, target: &Path, tool: &str) -> io::Result<bool> {
    // 旧 uv 根可能包含绝对路径环境，只迁工具版本，保留这些环境的原路径。
    let environments = tool == "uv"
        && ["tools", "bin", "python", "cache", "bundles"]
            .iter()
            .any(|name| source.join(name).is_dir());
    Ok(environments || (target.exists() && fs::read_dir(target)?.next().is_some()))
}

fn migrate_versions(source: &Path, target: &Path, tool: &str) -> io::Result<()> {
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        if semver::Version::parse(&entry.file_name().to_string_lossy()).is_ok() {
            super::runtime_migration_transaction::recover(
                &source.join(entry.file_name()),
                &entry.path(),
            )?;
        }
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if semver::Version::parse(&name.to_string_lossy()).is_err() {
            continue;
        }
        let destination = target.join(&name);
        if super::runtime_migration_transaction::recover(&entry.path(), &destination)? {
            continue;
        }
        if !destination.exists() {
            relocate(&entry.path(), &destination)?;
        }
    }
    let pointer = fs::read(source.join("current.json"))?;
    let next = target.join("current.json.next");
    fs::write(&next, pointer)?;
    fs::rename(next, target.join("current.json"))?;
    tracing::info!(tool, source = %source.display(), target = %target.display(),
        "[shared-runtime] migrated tool versions; retained legacy dependency directories");
    Ok(())
}

fn relocate(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("Runtime target has no parent"))?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".runtime-migration-{}", uuid::Uuid::new_v4()));
    let result = stage_tree(source, &staging, target)
        .and_then(|()| super::runtime_migration_transaction::prepare(source, &staging))
        .and_then(|()| fs::rename(&staging, target));
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    super::runtime_migration_transaction::publish(source, target)
}

fn stage_tree(source: &Path, target: &Path, final_target: &Path) -> io::Result<()> {
    let root = source.canonicalize()?;
    let roots = (root.as_path(), final_target);
    fs::create_dir_all(target)?;
    for entry in walkdir::WalkDir::new(source)
        .min_depth(1)
        .follow_links(false)
    {
        let entry = entry.map_err(io::Error::other)?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(io::Error::other)?;
        let destination = target.join(relative);
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_link(&metadata) {
            copy_link(entry.path(), &destination, roots)?;
        } else if metadata.is_dir() {
            fs::create_dir_all(&destination)?;
        } else if metadata.is_file() {
            copy_file(entry.path(), &destination)?;
        } else {
            return Err(io::Error::other(
                "Runtime contains an unsupported file type",
            ));
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> io::Result<()> {
    use sha2::{Digest, Sha256};
    fs::copy(source, target)?;
    let mut before = Sha256::new();
    let mut after = Sha256::new();
    io::copy(&mut fs::File::open(source)?, &mut before)?;
    io::copy(&mut fs::File::open(target)?, &mut after)?;
    if before.finalize() != after.finalize() {
        return Err(io::Error::other(format!(
            "Runtime copy verification failed: {}",
            source.display()
        )));
    }
    Ok(())
}

fn copy_link(source: &Path, target: &Path, roots: (&Path, &Path)) -> io::Result<()> {
    let (root, new_root) = roots;
    let resolved = source.canonicalize()?;
    let relative = resolved
        .strip_prefix(root)
        .map_err(|_| io::Error::other("Runtime link escapes its root"))?;
    let link_target = fs::read_link(source)?;
    let destination = if link_target.is_absolute() {
        new_root.join(relative)
    } else {
        link_target
    };
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(destination, target)
    }
    #[cfg(windows)]
    {
        if resolved.is_dir() {
            junction::create(new_root.join(relative), target)
        } else {
            std::os::windows::fs::symlink_file(destination, target)
        }
    }
}

fn same_directory(first: &Path, second: &Path) -> bool {
    first
        .canonicalize()
        .ok()
        .zip(second.canonicalize().ok())
        .is_some_and(|(first, second)| first == second)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
}
