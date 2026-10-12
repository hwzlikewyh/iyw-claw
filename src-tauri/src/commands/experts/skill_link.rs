use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::create_link_raw;
use crate::commands::acp::remove_skill_entry;

struct StagedLink {
    directory: PathBuf,
    link: PathBuf,
}

impl StagedLink {
    fn new(target: &Path) -> io::Result<Self> {
        let parent = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = parent.join(format!(".iyw-claw-skill-link-{}", uuid::Uuid::new_v4()));
        // 只有成功创建独占目录后，才拥有其中临时条目的清理权限。
        fs::create_dir(&directory).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "failed to create Skill link staging directory '{}' beside '{}': {error}",
                    directory.display(),
                    target.display()
                ),
            )
        })?;
        Ok(Self {
            link: directory.join("link"),
            directory,
        })
    }

    fn clear_link(&self) -> io::Result<()> {
        let metadata = match fs::symlink_metadata(&self.link) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if metadata.is_dir() || super::path_is_reparse_point(&self.link) {
            // 只删除链接或空目录，绝不递归进入中央 Skill。
            fs::remove_dir(&self.link)
        } else {
            fs::remove_file(&self.link)
        }
    }
}

impl Drop for StagedLink {
    fn drop(&mut self) {
        if let Err(error) = self.clear_link() {
            tracing::warn!(path = %self.link.display(), error = %error, "[skills] failed to clear staged directory link");
        }
        if let Err(error) = fs::remove_dir(&self.directory) {
            tracing::warn!(path = %self.directory.display(), error = %error, "[skills] retained link staging directory; check for a preserved previous Skill");
        }
    }
}

#[cfg(windows)]
pub(super) fn create_windows_link(source: &Path, target: &Path) -> io::Result<bool> {
    let source = fs::canonicalize(source)?;
    if !source.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Skill link source is not a directory",
        ));
    }
    match fs::symlink_metadata(target) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Skill link destination already exists; preserve it before publishing",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let staged = StagedLink::new(target)?;
    create_windows_staged_link(&source, &staged)?;
    let resolved = fs::canonicalize(&staged.link)?;
    if !super::paths_equivalent(&source, &resolved) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "staged Skill link does not resolve to the central source",
        ));
    }
    move_entry(&staged.link, target)?;
    Ok(false)
}

#[cfg(windows)]
fn create_windows_staged_link(source: &Path, staged: &StagedLink) -> io::Result<()> {
    let junction_error = match junction::create(source, &staged.link) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    // junction 可能已经创建空目录；清理后才能尝试目录符号链接。
    staged.clear_link().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "junction failed: {junction_error}; failed to clear its partial directory: {error}"
            ),
        )
    })?;
    std::os::windows::fs::symlink_dir(source, &staged.link).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "junction failed: {junction_error}; directory symbolic link failed: {error}. {}",
                windows_link_hint(&error)
            ),
        )
    })?;
    tracing::warn!(source = %source.display(), link = %staged.link.display(), error = %junction_error, "[skills] junction unavailable; using a directory symbolic link to the same source");
    Ok(())
}

#[cfg(windows)]
fn windows_link_hint(error: &io::Error) -> &'static str {
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PRIVILEGE_NOT_HELD};

    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_PRIVILEGE_NOT_HELD) => {
            "Check directory symbolic-link privileges; enable Windows Developer Mode when supported, or use an authorized elevated process."
        }
        Some(ERROR_ACCESS_DENIED) => {
            "Check Agent storage directory permissions and security software blocking directory links."
        }
        _ => "Check that the Agent storage filesystem supports directory links.",
    }
}

pub(super) fn replace_managed_copy_with_link(source: &Path, target: &Path) -> io::Result<bool> {
    if !source.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("managed Skill source is unavailable: {}", source.display()),
        ));
    }
    let staged = StagedLink::new(target)?;
    create_link_raw(source, &staged.link)?;
    let previous = staged.directory.join("previous");
    move_entry(target, &previous)?;
    if let Err(error) = move_entry(&staged.link, target) {
        let rollback = move_entry(&previous, target);
        return Err(io::Error::new(
            error.kind(),
            match rollback {
                Ok(()) => format!("failed to publish Skill link at '{}'; previous directory restored: {error}", target.display()),
                Err(rollback_error) => format!("failed to publish Skill link at '{}': {error}; restore failed: {rollback_error}; previous Skill retained at '{}'", target.display(), previous.display()),
            },
        ));
    }
    if let Err(error) = remove_skill_entry(&previous) {
        tracing::warn!(path = %previous.display(), error = %error, "[skills] directory link published but legacy managed copy cleanup failed");
    }
    Ok(false)
}

fn move_entry(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to move Skill directory entry '{}' to '{}': {error}",
                source.display(),
                target.display()
            ),
        )
    })
}
