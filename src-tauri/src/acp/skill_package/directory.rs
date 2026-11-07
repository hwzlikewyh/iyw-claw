use super::*;

const MANAGED_ENTRY_ALLOWANCE: usize = INTERNAL_MARKERS.len() + RUNTIME_ENV_DIR_NAMES.len();
// JSON转义最坏每个原字符占6字节；兼容头部不能突破原入口的有界增长。
const MAX_JSON_ESCAPE_BYTES: u64 = 6;
const MAX_COMPAT_ENTRY_GROWTH: u64 = MAX_SKILL_MD_BYTES * MAX_JSON_ESCAPE_BYTES;

pub(crate) fn validate_directory(
    root: &Path,
    expected: &str,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    validate_with_mode(root, expected, false)
}

pub(crate) fn validate_skill_directory(
    root: &Path,
    expected: &str,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    let package = validate_with_mode(root, expected, true)?;
    validate_skill_entry(&package.files)?;
    Ok(package)
}

fn validate_with_mode(
    root: &Path,
    expected: &str,
    skill: bool,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    let metadata =
        std::fs::symlink_metadata(root).map_err(|error| invalid_package(error.to_string()))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(invalid_package(
            "Plugin recovery root must be a real directory",
        ));
    }
    let canonical = root
        .canonicalize()
        .map_err(|error| invalid_package(error.to_string()))?;
    let mut state = DirectoryState {
        root: canonical.clone(),
        files: Vec::new(),
        expanded: 0,
        entries: 0,
        seen: BTreeSet::new(),
        directories: HashSet::new(),
        skill,
    };
    let mut pending = vec![canonical];
    while let Some(directory) = pending.pop() {
        for entry in
            std::fs::read_dir(directory).map_err(|error| invalid_package(error.to_string()))?
        {
            let path = entry
                .map_err(|error| invalid_package(error.to_string()))?
                .path();
            inspect_entry(&mut state, path, &mut pending)?;
        }
    }
    finish(state, expected)
}

fn finish(
    mut state: DirectoryState,
    expected: &str,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    state
        .files
        .sort_by(|left, right| normalized_path(&left.path).cmp(&normalized_path(&right.path)));
    let digest = hash_file_tree(&state.files);
    if digest != expected {
        return restore_skill_entry(state, expected);
    }
    confirmed(state.files, digest)
}

fn restore_skill_entry(
    mut state: DirectoryState,
    expected: &str,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    if state.skill {
        if let Some(entry) = state
            .files
            .iter_mut()
            .find(|file| file.path == Path::new("SKILL.md"))
        {
            let original = std::str::from_utf8(&entry.bytes)
                .ok()
                .and_then(super::super::skill_routing::original_legacy_skill_description_content);
            if let Some(original) = original {
                entry.bytes = original.into_bytes();
                entry.sha256 = hash_bytes(&entry.bytes);
                let digest = hash_file_tree(&state.files);
                if digest == expected {
                    return confirmed(state.files, digest);
                }
            }
        }
    }
    Err(invalid_package(
        "Installed file tree does not match the published version; repair the original package",
    ))
}

fn confirmed(
    files: Vec<PackageFile>,
    content_sha256: String,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    let size = files.iter().try_fold(0u64, |total, file| {
        total.checked_add(file.bytes.len() as u64)
    });
    if size.is_none() {
        return Err(invalid_package(
            "Restored Skill exceeds the package size limit",
        ));
    }
    Ok(ValidatedSkillPackage {
        files,
        content_sha256,
    })
}

struct DirectoryState {
    root: PathBuf,
    files: Vec<PackageFile>,
    expanded: u64,
    entries: usize,
    seen: BTreeSet<String>,
    directories: HashSet<String>,
    skill: bool,
}

fn inspect_entry(
    state: &mut DirectoryState,
    path: PathBuf,
    pending: &mut Vec<PathBuf>,
) -> Result<(), AppCommandError> {
    state.entries += 1;
    let allowance = if state.skill {
        MANAGED_ENTRY_ALLOWANCE
    } else {
        0
    };
    if state.entries > MAX_ARCHIVE_ENTRIES + allowance {
        return Err(invalid_package("Recovery contains too many entries"));
    }
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|error| invalid_package(error.to_string()))?;
    if is_link(&metadata) {
        return Err(invalid_package(
            "Recovery refuses symbolic links and reparse points",
        ));
    }
    let relative = path
        .strip_prefix(&state.root)
        .map_err(|_| invalid_package("Recovery path escapes its installation"))?;
    if relative.to_str().is_none() {
        return Err(invalid_package(
            "Installed package paths must be valid UTF-8",
        ));
    }
    if state.skill
        && RUNTIME_ENV_DIR_NAMES
            .iter()
            .any(|name| relative == Path::new(name))
    {
        return Ok(());
    }
    if state.skill && relative.to_str().is_some_and(is_internal_marker) {
        if !metadata.is_file() {
            return Err(invalid_package("Invalid managed Skill marker"));
        }
        return Ok(());
    }
    validate_path(&normalized_path(relative))?;
    if metadata.is_dir() {
        pending.push(path);
    } else {
        read_file(state, &path, &metadata)?;
    }
    Ok(())
}

pub(crate) fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn read_file(
    state: &mut DirectoryState,
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), AppCommandError> {
    if !metadata.is_file() || state.files.len() >= MAX_FILES {
        return Err(invalid_package(
            "Plugin recovery contains unsupported or excessive files",
        ));
    }
    let relative = path
        .strip_prefix(&state.root)
        .map_err(|_| invalid_package("Plugin recovery path escapes its root"))?;
    state.expanded = checked_size(state, relative, metadata.len())?;
    register_path(relative, &mut state.seen, &mut state.directories)?;
    let file = std::fs::File::open(path).map_err(|error| invalid_package(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(metadata.len() + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid_package(error.to_string()))?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid_package(
            "Plugin files changed during recovery validation",
        ));
    }
    state.files.push(PackageFile {
        path: relative.to_path_buf(),
        sha256: hash_bytes(&bytes),
        bytes,
    });
    Ok(())
}

fn checked_size(
    state: &DirectoryState,
    relative: &Path,
    length: u64,
) -> Result<u64, AppCommandError> {
    if state.skill
        && relative == Path::new("SKILL.md")
        && length > MAX_SKILL_MD_BYTES + MAX_COMPAT_ENTRY_GROWTH
    {
        return Err(invalid_package(
            "Installed Skill entry exceeds its compatibility size limit",
        ));
    }
    state
        .expanded
        .checked_add(length)
        .ok_or_else(|| invalid_package("Plugin recovery exceeds the size limit"))
}
