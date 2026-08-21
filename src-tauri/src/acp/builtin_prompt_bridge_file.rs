use std::fs;
use std::path::Path;

use crate::acp::builtin_agent_prompt::RenderedBuiltinPrompt;
use crate::acp::error::AcpError;
use sha2::{Digest, Sha256};

const BLOCK_START: &str = "<!-- IYW-CLAW BUILTIN PROMPT START";
const BLOCK_END: &str = "<!-- IYW-CLAW BUILTIN PROMPT END -->";

#[derive(Clone, Copy)]
struct BlockMetadata {
    created: bool,
    leading: usize,
    trailing: usize,
    eol: &'static str,
    hash: Option<[u8; 32]>,
}

struct ManagedBlock {
    start: usize,
    end: usize,
    metadata: Option<BlockMetadata>,
}

pub(super) fn upsert_managed_block(
    path: &Path,
    prompt: &RenderedBuiltinPrompt,
) -> Result<(), AcpError> {
    ensure_plain_path(path)?;
    let created = !path.try_exists().map_err(path_io(path))?;
    let raw = read(path)?;
    let (metadata, previous) = match find_managed_block(&raw)? {
        Some(block) => {
            removal_bounds(&raw, &block)?;
            (metadata_for_update(&raw, &block), Some(block))
        }
        None => (metadata_for_insert(&raw, created), None),
    };
    let block = render_block(prompt, metadata);
    let next = replace_or_append(&raw, previous.as_ref(), &block, metadata);
    write(path, &raw, &next)
}

pub(super) fn remove_managed_block(
    path: &Path,
    expected_hash: Option<&str>,
) -> Result<(), AcpError> {
    ensure_plain_path(path)?;
    let raw = read(path)?;
    let Some(block) = find_managed_block(&raw)? else {
        return Ok(());
    };
    ensure_block_integrity(&raw, &block, expected_hash)?;
    let (start, end) = removal_bounds(&raw, &block)?;
    let next = format!("{}{}", &raw[..start], &raw[end..]);
    if block.metadata.is_some_and(|value| value.created) && next.is_empty() {
        fs::remove_file(path).map_err(path_io(path))?;
        return Ok(());
    }
    write(path, &raw, &next)
}

pub(super) fn ensure_plain_path(path: &Path) -> Result<(), AcpError> {
    if !path.is_absolute() {
        return Err(injection_error(format!(
            "prompt bridge path must be absolute: {}",
            path.display()
        )));
    }
    for candidate in path.ancestors() {
        let metadata = match fs::symlink_metadata(candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(path_io(candidate)(error)),
        };
        if is_link_or_reparse(&metadata) {
            return Err(injection_error(format!(
                "prompt bridge path contains a link or reparse point: {}",
                candidate.display()
            )));
        }
        let expected = if candidate == path {
            metadata.is_file()
        } else {
            metadata.is_dir()
        };
        if !expected {
            return Err(injection_error(format!(
                "prompt bridge path has an invalid component: {}",
                candidate.display()
            )));
        }
    }
    Ok(())
}

fn find_managed_block(raw: &str) -> Result<Option<ManagedBlock>, AcpError> {
    let starts = raw.match_indices(BLOCK_START).collect::<Vec<_>>();
    let ends = raw.match_indices(BLOCK_END).collect::<Vec<_>>();
    if starts.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if starts.len() != 1 || ends.len() != 1 || starts[0].0 >= ends[0].0 {
        return Err(injection_error(
            "managed prompt markers are missing, duplicated, or out of order",
        ));
    }
    let start = starts[0].0;
    let end = ends[0].0 + BLOCK_END.len();
    let header_end = raw[start..]
        .find('\n')
        .map(|offset| start + offset)
        .unwrap_or(end);
    let metadata = parse_metadata(raw[start..header_end].trim_end_matches('\r'));
    Ok(Some(ManagedBlock {
        start,
        end,
        metadata,
    }))
}

fn metadata_for_insert(raw: &str, created: bool) -> BlockMetadata {
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let leading = if raw.is_empty() {
        0
    } else if raw.ends_with(eol) {
        1
    } else {
        2
    };
    BlockMetadata {
        created,
        leading,
        trailing: 1,
        eol,
        hash: None,
    }
}

fn metadata_for_update(raw: &str, block: &ManagedBlock) -> BlockMetadata {
    block.metadata.unwrap_or(BlockMetadata {
        created: false,
        leading: 0,
        trailing: 0,
        eol: if raw[block.start..block.end].contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        },
        hash: None,
    })
}

fn render_block(prompt: &RenderedBuiltinPrompt, metadata: BlockMetadata) -> String {
    let eol_name = if metadata.eol == "\r\n" { "crlf" } else { "lf" };
    let header = format!(
        "{BLOCK_START} sha256={} created={} leading={} trailing={} eol={eol_name} -->",
        prompt.hash,
        usize::from(metadata.created),
        metadata.leading,
        metadata.trailing,
    );
    let body = prompt.text.replace('\n', metadata.eol);
    format!("{header}{}{body}{}{BLOCK_END}", metadata.eol, metadata.eol)
}

fn replace_or_append(
    raw: &str,
    previous: Option<&ManagedBlock>,
    block: &str,
    metadata: BlockMetadata,
) -> String {
    match previous {
        Some(previous) => format!(
            "{}{}{}",
            &raw[..previous.start],
            block,
            &raw[previous.end..]
        ),
        None => format!(
            "{}{}{}{}",
            raw,
            metadata.eol.repeat(metadata.leading),
            block,
            metadata.eol.repeat(metadata.trailing)
        ),
    }
}

fn removal_bounds(raw: &str, block: &ManagedBlock) -> Result<(usize, usize), AcpError> {
    let Some(metadata) = block.metadata else {
        return Ok((block.start, block.end));
    };
    let leading = metadata.eol.repeat(metadata.leading);
    let trailing = metadata.eol.repeat(metadata.trailing);
    let start = block
        .start
        .checked_sub(leading.len())
        .ok_or_else(|| injection_error("managed prompt leading separator is outside the file"))?;
    let end = block
        .end
        .checked_add(trailing.len())
        .ok_or_else(|| injection_error("managed prompt trailing separator is outside the file"))?;
    if raw.get(start..block.start) != Some(leading.as_str())
        || raw.get(block.end..end) != Some(trailing.as_str())
    {
        return Err(injection_error(
            "managed prompt separators changed while the Agent was running",
        ));
    }
    Ok((start, end))
}

fn parse_metadata(header: &str) -> Option<BlockMetadata> {
    let value = |name: &str| {
        header
            .split_ascii_whitespace()
            .find_map(|part| part.strip_prefix(name))
    };
    let created = match value("created=")? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let leading = value("leading=")?
        .parse::<usize>()
        .ok()
        .filter(|v| *v <= 2)?;
    let trailing = value("trailing=")?
        .parse::<usize>()
        .ok()
        .filter(|v| *v <= 1)?;
    let eol = match value("eol=")? {
        "lf" => "\n",
        "crlf" => "\r\n",
        _ => return None,
    };
    let hash = parse_hash(value("sha256=")?)?;
    Some(BlockMetadata {
        created,
        leading,
        trailing,
        eol,
        hash: Some(hash),
    })
}

fn parse_hash(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn ensure_block_integrity(
    raw: &str,
    block: &ManagedBlock,
    expected_hash: Option<&str>,
) -> Result<(), AcpError> {
    let metadata = block.metadata.ok_or_else(|| {
        injection_error("managed prompt marker metadata is missing; refusing to delete")
    })?;
    let stored_hash = metadata.hash.ok_or_else(|| {
        injection_error("managed prompt marker hash is missing; refusing to delete")
    })?;
    let body_hash = block_body_hash(raw, block, metadata.eol)?;
    if body_hash != stored_hash {
        return Err(injection_error(
            "managed prompt body changed while the Agent was running; refusing to delete",
        ));
    }
    if let Some(expected_hash) = expected_hash {
        let expected = parse_hash(expected_hash)
            .ok_or_else(|| injection_error("expected managed prompt hash is invalid"))?;
        if expected != stored_hash {
            return Err(injection_error(
                "managed prompt hash does not belong to this connection; refusing to delete",
            ));
        }
    }
    Ok(())
}

fn block_body_hash(raw: &str, block: &ManagedBlock, eol: &str) -> Result<[u8; 32], AcpError> {
    let header_end = raw[block.start..]
        .find('\n')
        .map(|offset| block.start + offset)
        .ok_or_else(|| injection_error("managed prompt header has no body"))?;
    let body_start = header_end
        .checked_add(eol.len())
        .ok_or_else(|| injection_error("managed prompt body start overflowed"))?;
    let body_end = block
        .end
        .checked_sub(BLOCK_END.len())
        .ok_or_else(|| injection_error("managed prompt body end underflowed"))?;
    let body = raw
        .get(body_start..body_end)
        .and_then(|body| body.strip_suffix(eol))
        .ok_or_else(|| injection_error("managed prompt body separators changed"))?;
    let normalized = body.replace("\r\n", "\n");
    Ok(Sha256::digest(normalized.as_bytes()).into())
}

fn read(path: &Path) -> Result<String, AcpError> {
    crate::acp::provider_overlay_files::read_optional(path)
        .map_err(AcpError::BuiltinPromptInjection)
}

fn write(path: &Path, old: &str, new: &str) -> Result<(), AcpError> {
    crate::acp::provider_overlay_files::write_if_changed(path, old, new)
        .map_err(AcpError::BuiltinPromptInjection)
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(target_os = "windows"))]
    false
}

fn path_io(path: &Path) -> impl FnOnce(std::io::Error) -> AcpError + '_ {
    move |error| injection_error(format!("{}: {error}", path.display()))
}

fn injection_error(message: impl Into<String>) -> AcpError {
    AcpError::BuiltinPromptInjection(message.into())
}
