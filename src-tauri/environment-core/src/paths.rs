use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub struct Layout {
    pub root: PathBuf,
    pub runtime: PathBuf,
    pub inventory: PathBuf,
    pub staging: PathBuf,
    pub quarantine: PathBuf,
    pub logs: PathBuf,
}

impl Layout {
    pub fn resolve() -> Result<Self> {
        let root = dirs::home_dir()
            .context("operating-system user home is unavailable")?
            .join(".iyw-claw");
        #[cfg(debug_assertions)]
        let root = std::env::var_os("IYW_CLAW_ENVIRONMENT_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or(root);
        if !root.is_absolute() {
            anyhow::bail!("environment root must be absolute")
        }
        Ok(Self {
            runtime: root.join("runtime"),
            inventory: root.join("inventory"),
            staging: root.join("staging/environment"),
            quarantine: root.join("quarantine/environment"),
            logs: root.join("logs/environment"),
            root,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for path in [
            &self.runtime,
            &self.inventory,
            &self.staging,
            &self.quarantine,
            &self.logs,
            &self.root.join("config"),
            &self.root.join("plugins"),
            &self.root.join("browser/profiles"),
            &self.root.join("browser-extensions"),
        ] {
            fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
        }
        Ok(())
    }

    pub fn current_snapshot(&self) -> PathBuf {
        self.inventory.join("environment-current.json")
    }

    pub fn pending_transaction(&self) -> PathBuf {
        self.inventory.join("pending-environment.txt")
    }

    pub fn transaction_dir(&self, transaction: &str) -> Result<PathBuf> {
        Ok(self.staging.join(safe_segment(transaction, "transaction")?))
    }

    pub fn component_dir(&self, component: &str, version: &str, platform: &str) -> Result<PathBuf> {
        Ok(self
            .runtime
            .join(safe_segment(component, "component")?)
            .join(safe_segment(version, "version")?)
            .join(safe_segment(platform, "platform")?))
    }
}

pub fn safe_segment<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    if value.is_empty()
        || value.len() > 128
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', ':'])
        || value.ends_with([' ', '.'])
        || value.chars().any(char::is_control)
    {
        anyhow::bail!("{label} path segment is invalid")
    }
    Ok(value)
}

pub fn platform() -> (String, String, String) {
    let target = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86" => "x86",
        other => other,
    };
    (
        target.to_string(),
        arch.to_string(),
        format!("{target}-{arch}"),
    )
}

pub fn slash_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .context("path escaped the managed root")?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub fn from_slash(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || relative
            .split('/')
            .any(|part| safe_segment(part, "relative path").is_err())
    {
        anyhow::bail!("managed relative path is invalid")
    }
    Ok(relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part)))
}

pub fn ensure_within(root: &Path, path: &Path) -> Result<()> {
    let canonical_root = root.canonicalize()?;
    let mut existing = path;
    while !existing.exists() {
        existing = existing
            .parent()
            .context("managed path has no existing parent")?;
    }
    if !existing.canonicalize()?.starts_with(canonical_root) {
        anyhow::bail!("managed path resolves outside the environment root")
    }
    Ok(())
}
