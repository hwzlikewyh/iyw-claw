use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u8 = 1;
const MAX_IDENTITY_BYTES: u64 = 1024;
const IDENTITY_FILE: &str = "fingerprint.json";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    schema_version: u8,
    seed: u32,
}

pub(super) fn launch_args(profile: &Path, compatibility: Option<&str>) -> io::Result<String> {
    let seed = load_or_create(profile)?;
    let platform = match std::env::consts::OS {
        "macos" => "macos",
        value => value,
    };
    let fingerprint = format!("--fingerprint={seed},--fingerprint-platform={platform}");
    Ok(match compatibility.filter(|value| !value.is_empty()) {
        Some(args) => format!("{args},{fingerprint}"),
        None => fingerprint,
    })
}

fn load_or_create(profile: &Path) -> io::Result<u32> {
    let path = profile.join(IDENTITY_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.len() > MAX_IDENTITY_BYTES {
                return Err(invalid_identity());
            }
            let value: Identity =
                serde_json::from_slice(&fs::read(&path)?).map_err(|_| invalid_identity())?;
            if value.schema_version != SCHEMA_VERSION || value.seed == 0 {
                return Err(invalid_identity());
            }
            return Ok(value.seed);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let seed = (uuid::Uuid::new_v4().as_u128() as u32).max(1);
    let identity = Identity {
        schema_version: SCHEMA_VERSION,
        seed,
    };
    let bytes = serde_json::to_vec(&identity).map_err(io::Error::other)?;
    let mut temporary = tempfile::NamedTempFile::new_in(profile)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    Ok(seed)
}

fn invalid_identity() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "浏览器指纹配置损坏，已保留原文件；请检查 profile 中的 fingerprint.json",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_profile_reuses_identity_and_preserves_compatibility_args() {
        let profile = tempfile::tempdir().unwrap();
        let first = launch_args(profile.path(), Some("--disable-gpu")).unwrap();
        let second = launch_args(profile.path(), Some("--disable-gpu")).unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("--disable-gpu,--fingerprint="));
        assert!(!first.contains("--fingerprint=0,"));
    }

    #[test]
    fn malformed_identity_is_not_replaced() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join(IDENTITY_FILE);
        for contents in [
            "broken",
            r#"{"schemaVersion":1,"seed":0}"#,
            r#"{"schemaVersion":2,"seed":42}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(launch_args(profile.path(), None).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }
}
