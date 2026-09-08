use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::ReadDenyMatcher;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashSet;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

#[path = "deny_read_walker.rs"]
mod walker;

use walker::DirectoryScanMode;
use walker::collect_existing_glob_directory_matches;

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
/// already exist under their literal scan root; future exact paths are handled
/// later by materializing them before the deny ACE is applied.
pub fn resolve_windows_deny_read_paths(
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    cwd: &AbsolutePathBuf,
) -> Result<Vec<AbsolutePathBuf>, String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for path in file_system_sandbox_policy.get_unreadable_roots_with_cwd(cwd.as_path()) {
        push_absolute_path(&mut paths, &mut seen, path.into_path_buf())?;
    }

    let unreadable_globs = file_system_sandbox_policy.get_unreadable_globs_with_cwd(cwd.as_path());
    if unreadable_globs.is_empty() {
        return Ok(paths);
    }

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
    let Some(matcher) = ReadDenyMatcher::try_new(&glob_policy, cwd.as_path())? else {
        return Ok(paths);
    };

    let scan_plans = glob_scan_plans(
        &unreadable_globs,
        file_system_sandbox_policy.glob_scan_max_depth,
    )?;

    for scan_plan in scan_plans {
        if !scan_plan.root.exists() {
            continue;
        }

        let directory_scan_mode = if let Some(file_paths) = ripgrep_files(&scan_plan)? {
            for path in file_paths {
                if matcher.is_read_denied(&path) {
                    push_absolute_path(&mut paths, &mut seen, path)?;
                }
            }
            DirectoryScanMode::DirectoriesOnly
        } else {
            // Rebuild the complete accessible snapshot with the policy matcher
            // when ripgrep is missing or reports an incomplete traversal. Never
            // apply ACLs from a failed scan's partial stdout.
            DirectoryScanMode::IncludeAllFiles
        };

        collect_existing_glob_directory_matches(
            &scan_plan.root,
            &matcher,
            &mut paths,
            &mut seen,
            scan_plan.max_depth,
            directory_scan_mode,
        )?;
    }

    Ok(paths)
}

fn ripgrep_files(scan_plan: &GlobScanPlan) -> Result<Option<Vec<PathBuf>>, String> {
    let mut command = Command::new("rg");
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    command
        .arg("--files")
        .arg("--hidden")
        .arg("--no-ignore")
        .arg("--glob-case-insensitive")
        .arg("--null");
    if let Some(max_depth) = scan_plan.max_depth {
        command.arg("--max-depth").arg(max_depth.to_string());
    }
    for glob in &scan_plan.globs {
        command.arg("--glob").arg(glob);
    }
    command.arg("--").arg(&scan_plan.root);

    let output = match command.output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "failed to run bundled ripgrep for unreadable glob scan under {}: {err}",
                scan_plan.root.display()
            ));
        }
    };
    if !output.status.success() {
        if output.status.code() == Some(1) && output.stderr.is_empty() {
            return Ok(Some(Vec::new()));
        }
        if output.status.code() == Some(2) {
            // Ripgrep uses exit 2 for traversal errors, including protected
            // Windows directories. Its output may be incomplete. The caller
            // must enumerate again using the matcher-backed walker; policy
            // syntax has already been validated by ReadDenyMatcher::try_new.
            return Ok(None);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ripgrep unreadable glob scan failed under {}: {stderr}",
            scan_plan.root.display()
        ));
    }

    output
        .stdout
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = std::str::from_utf8(path).map_err(|err| {
                format!(
                    "ripgrep returned a non-UTF-8 path under {}: {err}",
                    scan_plan.root.display()
                )
            })?;
            let path = PathBuf::from(path);
            Ok(if path.is_absolute() {
                path
            } else {
                scan_plan.root.join(path)
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn push_absolute_path(
    paths: &mut Vec<AbsolutePathBuf>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
) -> Result<(), String> {
    let absolute_path = AbsolutePathBuf::from_absolute_path(dunce::simplified(&path))
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
    // Start scanning at the deepest literal directory prefix before the first
    // glob metacharacter. For example, `C:\repo\**\*.env` only scans `C:\repo`
    // instead of the current directory or drive root.
    let first_glob = pattern
        .char_indices()
        .find(|(_, ch)| matches!(ch, '*' | '?' | '['))
        .map(|(index, _)| index)
        .unwrap_or(pattern.len());
    let literal_prefix = &pattern[..first_glob];
    let Some(separator_index) = literal_prefix.rfind(['/', '\\']) else {
        return GlobScanPlan {
            root: PathBuf::from("."),
            max_depth: effective_glob_scan_max_depth(pattern, configured_max_depth),
            globs: vec![ripgrep_glob(pattern)],
        };
    };
    let pattern_suffix = &pattern[separator_index + 1..];
    let is_drive_root_separator = separator_index > 0
        && literal_prefix
            .as_bytes()
            .get(separator_index - 1)
            .is_some_and(|ch| *ch == b':');
    if separator_index == 0 || is_drive_root_separator {
        return GlobScanPlan {
            root: PathBuf::from(&literal_prefix[..=separator_index]),
            max_depth: effective_glob_scan_max_depth(pattern_suffix, configured_max_depth),
            globs: vec![ripgrep_glob(pattern_suffix)],
        };
    }
    GlobScanPlan {
        root: PathBuf::from(literal_prefix[..separator_index].to_string()),
        max_depth: effective_glob_scan_max_depth(pattern_suffix, configured_max_depth),
        globs: vec![ripgrep_glob(pattern_suffix)],
    }
}

fn ripgrep_glob(pattern: &str) -> String {
    let pattern = pattern.replace('\\', "/");
    let mut escaped = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();

    while let Some(ch) = chars.next() {
        if ch != '[' {
            escaped.push(ch);
            continue;
        }

        let mut class = String::new();
        let mut closed = false;
        for class_ch in chars.by_ref() {
            if class_ch == ']' {
                closed = true;
                break;
            }
            class.push(class_ch);
        }

        if closed {
            escaped.push('[');
            escaped.push_str(&class);
            escaped.push(']');
        } else {
            escaped.push_str(r"\[");
            escaped.push_str(&class);
        }
    }

    if escaped.starts_with("**/") {
        escaped
    } else {
        format!("**/{escaped}")
    }
}

fn effective_glob_scan_max_depth(
    pattern_suffix: &str,
    configured_max_depth: Option<usize>,
) -> Option<usize> {
    let components = pattern_suffix
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.contains(&"**") {
        return configured_max_depth;
    }
    Some(configured_max_depth.map_or(components.len(), |max_depth| {
        max_depth.min(components.len())
    }))
}
