use std::io;
use std::path::{Path, PathBuf};

const MAX_CONNECTION_ID_CHARS: usize = 128;

pub(super) struct TurnIdentity<'a> {
    pub connection_id: &'a str,
    pub conversation_id: i32,
    pub turn_generation: i64,
}

impl<'a> TurnIdentity<'a> {
    pub fn new(connection_id: &'a str, conversation_id: i32, turn_generation: i64) -> Self {
        Self {
            connection_id,
            conversation_id,
            turn_generation,
        }
    }
}

pub(super) fn turn_directory(identity: &TurnIdentity<'_>) -> io::Result<PathBuf> {
    if identity.conversation_id <= 0
        || identity.turn_generation <= 0
        || !safe_connection_id(identity.connection_id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid managed artifact path identity",
        ));
    }
    Ok(crate::paths::iyw_claw_task_artifacts_root()
        .join(identity.conversation_id.to_string())
        .join(identity.connection_id)
        .join(format!("turn-{}", identity.turn_generation)))
}

pub(super) async fn create_managed_directory(identity: &TurnIdentity<'_>) -> io::Result<()> {
    let root = crate::paths::iyw_claw_task_artifacts_root();
    tokio::fs::create_dir_all(&root).await?;
    let canonical_root = canonical_managed_root(&root).await?;
    let levels = [
        identity.conversation_id.to_string(),
        identity.connection_id.to_string(),
        format!("turn-{}", identity.turn_generation),
    ];
    let mut directory = root;
    let mut expected = canonical_root;
    for level in levels {
        directory.push(&level);
        expected.push(&level);
        create_level(&directory).await?;
        if tokio::fs::canonicalize(&directory).await? != expected {
            return Err(scope_escape_error());
        }
    }
    Ok(())
}

async fn canonical_managed_root(root: &Path) -> io::Result<PathBuf> {
    let parent = root.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "artifact root has no parent")
    })?;
    let name = root.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact root has no directory name",
        )
    })?;
    let expected = tokio::fs::canonicalize(parent).await?.join(name);
    let canonical = tokio::fs::canonicalize(root).await?;
    if canonical == expected {
        return Ok(canonical);
    }
    Err(scope_escape_error())
}

async fn create_level(directory: &Path) -> io::Result<()> {
    match tokio::fs::create_dir(directory).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn safe_connection_id(connection_id: &str) -> bool {
    !connection_id.is_empty()
        && connection_id.len() <= MAX_CONNECTION_ID_CHARS
        && connection_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn scope_escape_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "managed artifact directory escaped its turn scope",
    )
}
