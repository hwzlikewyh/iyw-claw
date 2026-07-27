//! Optional external agent-CLI transcript handling (the "include conversation
//! content" toggle).
//!
//! Backup packs each source under `external/<agent>/`. Restore never silently
//! clobbers a live CLI directory: callers either drop these (Skip), extract
//! them to a safe side folder (SideLocation), or — only with an explicit
//! conflict decision — write them back to their original locations
//! (OriginalLocations), where any file that already exists is skipped unless
//! the user authorized overwriting.

use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use zip::ZipArchive;

use crate::app_error::AppCommandError;
use crate::parsers::{external_transcript_sources, ExternalSource};

use super::archive::{ArchiveBuilder, ProgressFn};
use super::restore::ConflictPolicy;
use super::{cancelled_error, unknown_format_error};

/// A staged external file whose target already exists on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalConflict {
    pub agent: String,
    /// Path inside the archive (e.g. `external/claude/projects/foo.jsonl`).
    pub archive_path: String,
    /// Absolute live path the entry would overwrite.
    pub target_path: String,
    pub target_size: Option<u64>,
}

/// Pack external transcript trees into the archive. Returns whether anything
/// was added (drives the manifest's `includes_external_transcripts`).
pub fn add_external_sources(
    builder: &mut ArchiveBuilder,
    cancel: &CancellationToken,
    progress: &mut ProgressFn<'_>,
) -> Result<bool, AppCommandError> {
    let mut packed = false;
    for src in external_transcript_sources() {
        if cancel.is_cancelled() {
            return Err(cancelled_error());
        }
        if !src.root.exists() {
            continue;
        }
        let prefix = format!("external/{}", src.agent);
        if src.is_file {
            let name = src
                .root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("data");
            builder.add_file(&format!("{prefix}/{name}"), &src.root, cancel, progress)?;
            packed = true;
        } else {
            // Honor the per-source allowlist so a mixed base dir (e.g. Gemini's,
            // which holds credentials next to transcripts) only contributes its
            // transcript/session subtrees.
            let include_top = src.include_top;
            let exclude = move |rel: &Path| match include_top {
                None => false,
                Some(allow) => match rel.components().next() {
                    Some(std::path::Component::Normal(first)) => {
                        let first = first.to_string_lossy();
                        !allow.iter().any(|a| *a == first)
                    }
                    _ => true,
                },
            };
            builder.add_dir(&prefix, &src.root, &exclude, cancel, progress)?;
            packed = true;
        }
    }
    Ok(packed)
}

/// Scan a (plaintext) backup ZIP for external entries whose live target already
/// exists, so the UI can surface conflicts before any write.
pub fn scan_external_conflicts(zip_path: &Path) -> Result<Vec<ExternalConflict>, AppCommandError> {
    scan_external_conflicts_with_sources(zip_path, &external_transcript_sources())
}

fn scan_external_conflicts_with_sources(
    zip_path: &Path,
    sources: &[ExternalSource],
) -> Result<Vec<ExternalConflict>, AppCommandError> {
    let f = File::open(zip_path).map_err(AppCommandError::io)?;
    let mut ar = ZipArchive::new(BufReader::new(f)).map_err(|_| unknown_format_error())?;

    let mut conflicts = Vec::new();
    for i in 0..ar.len() {
        let entry = ar.by_index(i).map_err(|_| unknown_format_error())?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let rel_str = to_slash(&rel);
        let Some((agent, _base, target)) = map_external_to_target(&rel_str, sources) else {
            continue;
        };
        // `symlink_metadata` matches the restore-side conflict test exactly, so
        // the preview reports dangling symlinks too (they are conflicts on
        // restore).
        if let Ok(meta) = std::fs::symlink_metadata(&target) {
            conflicts.push(ExternalConflict {
                agent,
                archive_path: rel_str,
                target_size: Some(meta.len()),
                target_path: target.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(conflicts)
}

/// Write already-extracted `external/<agent>/…` files from `staged_external`
/// back to their original CLI locations, honoring `policy`. Returns the live
/// paths that were skipped because they already existed and overwrite was not
/// authorized. Never overwrites a conflicting file under `SkipExisting`.
pub fn restore_external_from_staging(
    staged_external: &Path,
    policy: ConflictPolicy,
    cancel: &CancellationToken,
) -> Result<Vec<String>, AppCommandError> {
    restore_external_with_sources(
        staged_external,
        &external_transcript_sources(),
        policy,
        cancel,
    )
}

fn restore_external_with_sources(
    staged_external: &Path,
    sources: &[ExternalSource],
    policy: ConflictPolicy,
    cancel: &CancellationToken,
) -> Result<Vec<String>, AppCommandError> {
    let mut skipped = Vec::new();

    for entry in walkdir::WalkDir::new(staged_external).follow_links(false) {
        if cancel.is_cancelled() {
            return Err(cancelled_error());
        }
        let entry = entry.map_err(|e| {
            AppCommandError::io_error("Walk staged transcripts").with_detail(e.to_string())
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        // Reconstruct the in-archive path (`external/<agent>/<rest>`) from the
        // staging-relative path so the same mapping as the scan applies.
        let rel = match entry.path().strip_prefix(staged_external) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let archive_path = format!("external/{}", to_slash(rel));
        // `map_external_to_target` enforces the per-agent allowlist + file-only
        // constraints, so a crafted archive entry (e.g. `external/gemini/
        // oauth_creds.json`) is dropped here rather than written to a live
        // config path.
        let Some((_agent, base, target)) = map_external_to_target(&archive_path, sources) else {
            continue;
        };
        match restore_one(entry.path(), &base, &target, policy) {
            FileOutcome::Written => {}
            FileOutcome::Skipped => skipped.push(target.to_string_lossy().into_owned()),
            FileOutcome::Failed => { /* logged in restore_one; non-fatal */ }
        }
    }
    Ok(skipped)
}

enum FileOutcome {
    Written,
    Skipped,
    Failed,
}

/// Place one staged file at `target` (which must live under `base`), never
/// leaving a partial file at the final path, never clobbering an existing file
/// under `SkipExisting`, and never following a symlinked parent component out
/// of `base`.
fn restore_one(src: &Path, base: &Path, target: &Path, policy: ConflictPolicy) -> FileOutcome {
    let exists = std::fs::symlink_metadata(target).is_ok();
    if exists && policy == ConflictPolicy::SkipExisting {
        return FileOutcome::Skipped;
    }
    let Some(parent) = target.parent() else {
        return FileOutcome::Failed;
    };
    // Refuse to write if any existing component between `base` and the target's
    // parent is a symlink — otherwise `create_dir_all`/rename would follow it
    // and write outside the agent's tree.
    if !parent_chain_is_safe(base, parent) {
        tracing::warn!(
            "[RESTORE] external: symlinked parent under {}, skipping {}",
            base.display(),
            target.display()
        );
        return FileOutcome::Failed;
    }
    if let Err(e) = std::fs::create_dir_all(parent) {
        tracing::error!("[RESTORE] external: mkdir {} failed: {e}", parent.display());
        return FileOutcome::Failed;
    }

    match policy {
        ConflictPolicy::SkipExisting => {
            // Atomic no-clobber: `create_new` fails if the path appeared in a
            // race. On a copy failure, remove the partial file we just created
            // so no half-written transcript is left at the live path.
            match OpenOptions::new().write(true).create_new(true).open(target) {
                Ok(mut out) => {
                    match File::open(src).and_then(|mut i| std::io::copy(&mut i, &mut out)) {
                        Ok(_) => FileOutcome::Written,
                        Err(e) => {
                            tracing::error!(
                                "[RESTORE] external: write {} failed: {e}",
                                target.display()
                            );
                            let _ = std::fs::remove_file(target);
                            FileOutcome::Failed
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => FileOutcome::Skipped,
                Err(e) => {
                    tracing::error!(
                        "[RESTORE] external: create {} failed: {e}",
                        target.display()
                    );
                    FileOutcome::Failed
                }
            }
        }
        ConflictPolicy::Overwrite => {
            // Write to a same-dir temp file, then publish by rename so the final
            // path never holds a partially-written file. The temp is cleaned up
            // on any failure.
            let tmp = parent.join(format!(
                ".iyw-claw-ext-{}.part",
                uuid::Uuid::new_v4().simple()
            ));
            let write = (|| -> std::io::Result<()> {
                let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
                let mut input = File::open(src)?;
                std::io::copy(&mut input, &mut out)?;
                Ok(())
            })();
            if let Err(e) = write {
                tracing::error!(
                    "[RESTORE] external: stage temp for {} failed: {e}",
                    target.display()
                );
                let _ = std::fs::remove_file(&tmp);
                return FileOutcome::Failed;
            }
            // rename() can't replace an existing file on Windows; remove the
            // existing entry (file or symlink) first.
            if exists {
                let _ = std::fs::remove_file(target);
            }
            if let Err(e) = std::fs::rename(&tmp, target) {
                tracing::error!(
                    "[RESTORE] external: publish {} failed: {e}",
                    target.display()
                );
                let _ = std::fs::remove_file(&tmp);
                return FileOutcome::Failed;
            }
            FileOutcome::Written
        }
    }
}

/// Map an `external/<agent>/<rest>` archive path to `(agent, base, live_target)`,
/// re-applying the SAME constraints used at backup time so a crafted archive
/// can't smuggle a non-transcript path into a live config location:
/// - the agent must be a known source;
/// - a file source accepts only its exact filename;
/// - a dir source with an `include_top` allowlist accepts only those top dirs;
/// - traversal components are rejected.
fn map_external_to_target(
    archive_path: &str,
    sources: &[ExternalSource],
) -> Option<(String, PathBuf, PathBuf)> {
    let rest = archive_path.strip_prefix("external/")?;
    let (agent, sub) = rest.split_once('/')?;
    let src = sources.iter().find(|s| s.agent == agent)?;

    // Reject traversal / non-normal components up front.
    let segs: Vec<&str> = sub.split('/').collect();
    if segs.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return None;
    }

    if src.is_file {
        // Only the source file's own name is allowed (e.g. `opencode.db`).
        let fname = src.root.file_name()?.to_str()?;
        if segs.as_slice() != [fname] {
            return None;
        }
        return Some((agent.to_string(), src.restore_base(), src.root.clone()));
    }

    if let Some(allow) = src.include_top {
        let first = segs.first()?;
        if !allow.iter().any(|a| a == first) {
            return None;
        }
    }

    let base = src.restore_base();
    let mut target = base.clone();
    for seg in &segs {
        target.push(seg);
    }
    Some((agent.to_string(), base, target))
}

/// True if no existing component between `base` (exclusive) and `dir`
/// (inclusive) is a symlink, and `dir` is actually under `base`. Used to refuse
/// writing through a symlinked parent that escapes the agent's tree.
fn parent_chain_is_safe(base: &Path, dir: &Path) -> bool {
    let Ok(rel) = dir.strip_prefix(base) else {
        return false;
    };
    let mut cur = base.to_path_buf();
    for comp in rel.components() {
        match comp {
            std::path::Component::Normal(s) => cur.push(s),
            // Any non-normal component (shouldn't occur post-mapping) is unsafe.
            _ => return false,
        }
        if let Ok(meta) = std::fs::symlink_metadata(&cur) {
            if meta.file_type().is_symlink() {
                return false;
            }
        }
    }
    true
}

fn to_slash(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

