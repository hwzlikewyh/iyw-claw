use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::ReadDenyMatcher;
use codex_protocol::permissions::ResolvedFileSystemPermissions;
use codex_protocol::permissions::windows_deny_read_glob_scan;
use codex_utils_absolute_path::AbsolutePathBuf;
use ignore::overrides::OverrideBuilder;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
#[cfg(windows)]
use std::time::Instant;

#[derive(Debug, Eq, PartialEq)]
struct GlobScanPlan {
    root: PathBuf,
    max_depth: Option<usize>,
    globs: Vec<String>,
}

/// Resolve split filesystem `None` read entries into concrete Windows ACL targets.
///
/// Windows ACLs do not understand Codex filesystem glob patterns directly. Exact
/// unreadable roots can be passed through as-is, including paths that do not
/// exist yet. Glob entries are snapshot-expanded to the files/directories that
/// already exist under their literal scan root. Directory links below that root
/// are not traversed; explicitly linked scan roots are followed. Future exact
/// paths are materialized later before the deny ACE is applied.
pub fn resolve_windows_deny_read_paths(
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    cwd: &AbsolutePathBuf,
) -> Result<Vec<AbsolutePathBuf>, String> {
    expand_deny_read_paths(
        &file_system_sandbox_policy.get_unreadable_roots_with_cwd(cwd),
        &file_system_sandbox_policy.get_unreadable_globs_with_cwd(cwd),
        file_system_sandbox_policy.glob_scan_max_depth,
        cwd,
    )
}

/// Expand the denies from an already resolved filesystem policy into Windows ACL targets.
pub fn expand_windows_deny_read_paths(
    permissions: &ResolvedFileSystemPermissions,
    cwd: &AbsolutePathBuf,
) -> Result<Vec<AbsolutePathBuf>, String> {
    expand_deny_read_paths(
        &permissions.unreadable_roots,
        &permissions.unreadable_globs,
        permissions.glob_scan_max_depth,
        cwd,
    )
}

fn expand_deny_read_paths(
    unreadable_roots: &[AbsolutePathBuf],
    unreadable_globs: &[String],
    glob_scan_max_depth: Option<usize>,
    cwd: &AbsolutePathBuf,
) -> Result<Vec<AbsolutePathBuf>, String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for path in unreadable_roots {
        push_absolute_path(&mut paths, &mut seen, path.as_path())?;
    }

    if unreadable_globs.is_empty() {
        return Ok(paths);
    }

    #[cfg(windows)]
    let started = Instant::now();
    let glob_policy = FileSystemSandboxPolicy::restricted(
        unreadable_globs
            .iter()
            .map(|pattern| FileSystemSandboxEntry {
                path: FileSystemPath::GlobPattern {
                    pattern: pattern.clone(),
                },
                access: FileSystemAccessMode::Deny,
                missing_path_behavior: None,
            })
            .collect(),
    );
    let Some(matcher) = ReadDenyMatcher::try_new_for_local_paths(&glob_policy, cwd.as_path())?
    else {
        return Ok(paths);
    };

    let scan_plans = glob_scan_plans(unreadable_globs, glob_scan_max_depth)?;
    #[cfg(windows)]
    let roots = scan_plans.len();

    for scan_plan in scan_plans {
        if !scan_plan.root.exists() {
            continue;
        }

        // Snapshot matching entries without following directory links below the scan root.
        let mut filter = OverrideBuilder::new(&scan_plan.root);
        filter.allow_unclosed_class(/*yes*/ true);
        filter
            .case_insensitive(/*yes*/ true)
            .map_err(|err| err.to_string())?;
        for glob in &scan_plan.globs {
            filter.add(glob).map_err(|err| err.to_string())?;
        }
        let filter = filter.build().map_err(|err| err.to_string())?;

        for entry in ignore::WalkBuilder::new(&scan_plan.root)
            .standard_filters(/*yes*/ false)
            .follow_links(/*yes*/ false)
            .max_depth(scan_plan.max_depth)
            .overrides(filter)
            .build()
        {
            let entry = entry.and_then(|entry| {
                if entry.path_is_symlink() && matcher.is_local_path_read_denied(entry.path()) {
                    std::fs::metadata(entry.path()).map_err(ignore::Error::Io)?;
                }
                Ok(entry)
            });
            let entry = match entry {
                Ok(entry) => entry,
                Err(err)
                    if err.io_error().is_some_and(|err| {
                        matches!(
                            err.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        )
                    }) =>
                {
                    continue;
                }
                Err(err) => {
                    return Err(format!(
                        "failed to enumerate unreadable glob paths under {}: {err}",
                        scan_plan.root.display()
                    ));
                }
            };
            if matcher.is_local_path_read_denied(entry.path()) {
                push_absolute_path(&mut paths, &mut seen, entry.into_path())?;
            }
        }
    }

    #[cfg(windows)]
    {
        let elapsed_ms = started.elapsed().as_millis();
        let patterns = unreadable_globs.len();
        crate::log_runtime_note(&format!(
            "MXC deny glob expansion elapsed_ms={elapsed_ms} patterns={patterns} roots={roots} glob_scan_max_depth={glob_scan_max_depth:?}"
        ));
    }
    Ok(paths)
}

fn push_absolute_path(
    paths: &mut Vec<AbsolutePathBuf>,
    seen: &mut HashSet<PathBuf>,
    path: impl AsRef<Path>,
) -> Result<(), String> {
    let absolute_path = AbsolutePathBuf::from_absolute_path(dunce::simplified(path.as_ref()))
        .map_err(|err| err.to_string())?;
    if seen.insert(absolute_path.to_path_buf()) {
        paths.push(absolute_path);
    }
    Ok(())
}

fn glob_scan_plans(
    patterns: &[String],
    configured_max_depth: Option<usize>,
) -> Result<Vec<GlobScanPlan>, String> {
    let mut scan_plans: Vec<GlobScanPlan> = Vec::new();

    for pattern in patterns {
        let mut scan_plan = glob_scan_plan(pattern, configured_max_depth);
        if scan_plan.max_depth.is_none() && scan_plan.root.parent().is_none() {
            return Err(format!(
                "unreadable glob `{pattern}` cannot be safely expanded from a filesystem root without `glob_scan_max_depth`; configure `glob_scan_max_depth` or use a non-root directory prefix"
            ));
        }

        if let Some(existing) = scan_plans
            .iter_mut()
            .find(|existing| existing.root == scan_plan.root)
        {
            existing.max_depth = match (existing.max_depth, scan_plan.max_depth) {
                (Some(existing_depth), Some(new_depth)) => Some(existing_depth.max(new_depth)),
                _ => None,
            };
            existing.globs.append(&mut scan_plan.globs);
        } else {
            scan_plans.push(scan_plan);
        }
    }

    Ok(scan_plans)
}

fn glob_scan_plan(pattern: &str, configured_max_depth: Option<usize>) -> GlobScanPlan {
    let scan = windows_deny_read_glob_scan(pattern, configured_max_depth);
    GlobScanPlan {
        root: PathBuf::from(scan.root),
        max_depth: scan.max_depth,
        globs: vec![file_filter_glob(scan.pattern_suffix)],
    }
}

fn file_filter_glob(pattern: &str) -> String {
    let pattern = pattern.replace('\\', "/");
    if pattern.starts_with("**/") {
        pattern
    } else {
        format!("**/{pattern}")
    }
}
