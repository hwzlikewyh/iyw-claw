use clap::Args;
use handler_common::{HandlerError, OutputFormat};
use std::path::{Path, PathBuf};

/// Install officecli binary, skills, and MCP configuration
#[derive(Args)]
pub struct InstallCommand {
    /// Target tool: all, claude, cursor, vscode, lms (default: all)
    pub target: Option<String>,

    /// Dry run: show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Override install prefix (for testing; defaults to standard paths)
    #[arg(long)]
    pub prefix: Option<String>,
}

pub fn handle_install(cmd: InstallCommand, _format: OutputFormat) -> Result<String, HandlerError> {
    let target = cmd.target.unwrap_or_else(|| "all".to_string());
    let dry_run = cmd.dry_run;
    let prefix = cmd.prefix.clone();
    let valid_targets = ["all", "claude", "claude-code", "cursor", "vscode", "lms"];
    if !valid_targets.contains(&target.as_str()) {
        return Err(HandlerError::InvalidArgument(format!(
            "unknown target '{}'. Valid: {}",
            target,
            valid_targets.join(", ")
        )));
    }

    let mut messages = Vec::new();

    // 1. Install binary
    let bin_msg = install_binary(dry_run, &prefix)?;
    messages.push(bin_msg);

    // 2. Install skills
    let skilled = install_skills(&target, dry_run, &prefix)?;
    let has_skills = !skilled.is_empty();
    for s in skilled {
        messages.push(s);
    }

    // 3. Install MCP fallback
    let mcp_installed = install_mcp_fallback(&target, has_skills, dry_run, &prefix)?;
    if let Some(ref msg) = mcp_installed {
        messages.push(msg.clone());
    }

    // Exit 1 when a specific named target matched neither skills nor MCP
    if target != "all" && !has_skills && mcp_installed.is_none() {
        return Err(HandlerError::InvalidArgument(format!(
            "target '{}' not recognized for skill or MCP installation",
            target
        )));
    }

    Ok(messages.join("\n"))
}

// ─── Binary Installation ──────────────────────────────────────

fn install_binary(dry_run: bool, prefix: &Option<String>) -> Result<String, HandlerError> {
    let exe = std::env::current_exe().map_err(|e| {
        HandlerError::OperationFailed(format!("cannot determine current exe: {}", e))
    })?;

    let dest = bin_install_path(prefix);

    // Skip if already there and identical
    if dest.exists() && files_identical(&exe, &dest) {
        return Ok(format!("Binary already installed: {}", dest.display()));
    }

    if dry_run {
        return Ok(format!(
            "[DRY RUN] Would copy {} → {}",
            exe.display(),
            dest.display()
        ));
    }

    // Ensure parent dir exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            HandlerError::OperationFailed(format!("failed to create {}: {}", parent.display(), e))
        })?;
    }

    std::fs::copy(&exe, &dest).map_err(|e| {
        HandlerError::OperationFailed(format!(
            "failed to copy {} → {}: {}",
            exe.display(),
            dest.display(),
            e
        ))
    })?;

    Ok(format!("Installed binary: {}", dest.display()))
}

fn bin_install_path(prefix: &Option<String>) -> PathBuf {
    if let Some(p) = prefix {
        return PathBuf::from(p).join(if cfg!(windows) {
            "officecli.exe"
        } else {
            "officecli"
        });
    }
    if cfg!(windows) {
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"C:\OfficeCli".to_string()))
            .join("officecli.exe")
    } else {
        PathBuf::from(home_dir()).join(".local/bin/officecli")
    }
}

// ─── Skills Installation ──────────────────────────────────────

fn install_skills(
    target: &str,
    dry_run: bool,
    _prefix: &Option<String>,
) -> Result<Vec<String>, HandlerError> {
    let skill_targets: &[(&str, &str)] = &[
        ("claude", ".claude/skills/officecli"),
        ("claude-code", ".claude/skills/officecli"),
        ("cursor", ".cursor/skills/officecli"),
    ];

    let mut installed = Vec::new();
    let home = home_dir();

    for (name, skill_dir) in skill_targets {
        if target != "all" && target != *name {
            continue;
        }

        let dest = PathBuf::from(&home).join(skill_dir);

        if dry_run {
            installed.push(format!(
                "[DRY RUN] Would install skill '{}' → {}",
                name,
                dest.display()
            ));
            continue;
        }

        std::fs::create_dir_all(&dest).map_err(|e| {
            HandlerError::OperationFailed(format!("failed to create {}: {}", dest.display(), e))
        })?;

        // Copy SKILL.md from the skills directory next to the binary
        if let Some(skill_src) = find_skill_source(name) {
            let skill_dest = dest.join("SKILL.md");
            std::fs::copy(&skill_src, &skill_dest).map_err(|e| {
                HandlerError::OperationFailed(format!("failed to copy skill: {}", e))
            })?;
            installed.push(format!("Installed skill '{}' → {}", name, dest.display()));
        } else {
            installed.push(format!(
                "Skill '{}' directory created (no bundled SKILL.md found)",
                name
            ));
        }
    }

    Ok(installed)
}

fn find_skill_source(name: &str) -> Option<PathBuf> {
    // Look for skills/<name>/SKILL.md next to the executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let skill_path = exe_dir.join("skills").join(name).join("SKILL.md");
            if skill_path.exists() {
                return Some(skill_path);
            }
            // Also check parent dirs (dev mode)
            if let Some(parent) = exe_dir.parent() {
                let skill_path = parent.join("skills").join(name).join("SKILL.md");
                if skill_path.exists() {
                    return Some(skill_path);
                }
            }
        }
    }
    None
}

// ─── MCP Fallback Installation ────────────────────────────────

fn install_mcp_fallback(
    target: &str,
    _has_skills: bool,
    dry_run: bool,
    _prefix: &Option<String>,
) -> Result<Option<String>, HandlerError> {
    // MCP targets: vscode, lms — tools without skill aliases
    let mcp_targets: &[(&str, &str)] = &[("vscode", "vscode"), ("lms", "lms")];

    for (name, _) in mcp_targets {
        if target != "all" && target != *name {
            continue;
        }

        let config_path = mcp_config_path(name);
        if dry_run {
            return Ok(Some(format!(
                "[DRY RUN] Would write MCP config for '{}' at {}",
                name,
                config_path.display()
            )));
        }

        // Write a minimal MCP config
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                HandlerError::OperationFailed(format!(
                    "failed to create {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let mcp_config = serde_json::json!({
            "mcpServers": {
                "officecli": {
                    "command": "officecli",
                    "args": ["mcp"]
                }
            }
        });

        // Merge with existing config if present
        let existing: serde_json::Value = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        let merged = merge_mcp_config(&existing, &mcp_config);
        let content = serde_json::to_string_pretty(&merged)
            .map_err(|e| HandlerError::OperationFailed(e.to_string()))?;

        std::fs::write(&config_path, content).map_err(|e| {
            HandlerError::OperationFailed(format!(
                "failed to write {}: {}",
                config_path.display(),
                e
            ))
        })?;

        return Ok(Some(format!(
            "Installed MCP config for '{}' at {}",
            name,
            config_path.display()
        )));
    }

    Ok(None)
}

fn mcp_config_path(target: &str) -> PathBuf {
    let home = home_dir();
    match target {
        "vscode" => {
            #[cfg(target_os = "macos")]
            {
                PathBuf::from(home).join("Library/Application Support/Code/User/mcp.json")
            }
            #[cfg(target_os = "linux")]
            {
                PathBuf::from(home).join(".config/Code/User/mcp.json")
            }
            #[cfg(target_os = "windows")]
            {
                PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
                    .join("Code/User/mcp.json")
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                PathBuf::from(home).join(".config/Code/User/mcp.json")
            }
        }
        "lms" => PathBuf::from(home).join(".cache/lm-studio/mcp.json"),
        _ => PathBuf::from(home).join(".config/officecli/mcp.json"),
    }
}

fn merge_mcp_config(existing: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let mut result = existing.clone();
    if let Some(existing_obj) = result.as_object_mut() {
        if let Some(new_obj) = new.as_object() {
            for (key, value) in new_obj {
                existing_obj.insert(key.clone(), value.clone());
            }
        }
    } else {
        result = new.clone();
    }
    result
}

// ─── Helpers ──────────────────────────────────────────────────

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string())
}

fn files_identical(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let Ok(mut fa) = std::fs::File::open(a) else {
        return false;
    };
    let Ok(mut fb) = std::fs::File::open(b) else {
        return false;
    };

    let meta_a = std::fs::metadata(a);
    let meta_b = std::fs::metadata(b);
    if let (Ok(ma), Ok(mb)) = (meta_a, meta_b) {
        if ma.len() != mb.len() {
            return false;
        }
    }

    let mut buf_a = Vec::new();
    let mut buf_b = Vec::new();
    if fa.read_to_end(&mut buf_a).is_err() || fb.read_to_end(&mut buf_b).is_err() {
        return false;
    }
    buf_a == buf_b
}
