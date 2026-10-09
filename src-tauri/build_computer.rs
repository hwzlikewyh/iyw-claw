use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn export_identity() {
    export_signing_requirements();
    let mut files = source_files(Path::new("src/computer"));
    files.extend(source_files(Path::new("src/commands/parts/computer")));
    files.extend(source_files(Path::new("src/acp/parts/computer_tools")));
    files.extend(
        [
            "src/commands/computer.rs",
            "src/acp/computer_tools.rs",
            "src/bin_targets/iyw_computer_helper.rs",
        ]
        .map(PathBuf::from),
    );
    files.sort_by_key(|file| file.to_string_lossy().replace('\\', "/"));
    let mut hash = Sha256::new();
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        hash.update(file.to_string_lossy().replace('\\', "/").as_bytes());
        hash.update(std::fs::read(&file).expect("Computer Use source unavailable"));
    }
    println!(
        "cargo:rustc-env=IYW_CLAW_COMPUTER_SOURCE={:x}",
        hash.finalize()
    );
    for name in [
        "IYW_CLAW_COMPUTER_TEAM_ID",
        "IYW_CLAW_COMPUTER_HELPER_REQUIREMENT",
        "IYW_CLAW_COMPUTER_PEER_REQUIREMENT",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        if let Ok(value) = std::env::var(name) {
            println!("cargo:rustc-env={name}={value}");
        }
    }
}

fn export_signing_requirements() {
    println!("cargo:rerun-if-env-changed=APPLE_SIGNING_IDENTITY");
    println!("cargo:rerun-if-env-changed=APPLE_TEAM_ID");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let identity = std::env::var("APPLE_SIGNING_IDENTITY").unwrap_or_default();
    let team = std::env::var("APPLE_TEAM_ID").ok().or_else(|| {
        let team = identity.rsplit_once('(')?.1.strip_suffix(')')?;
        (team.len() == 10 && team.bytes().all(|ch| ch.is_ascii_alphanumeric()))
            .then(|| team.to_string())
    });
    let Some(team) = team else {
        return;
    };
    let requirement = |id: &str| {
        format!("identifier \"{id}\" and anchor apple generic and certificate leaf[subject.OU] = \"{team}\"")
    };
    for (name, value) in [
        ("IYW_CLAW_COMPUTER_TEAM_ID", team.clone()),
        (
            "IYW_CLAW_COMPUTER_HELPER_REQUIREMENT",
            requirement("app.iywclaw.computer-helper"),
        ),
        (
            "IYW_CLAW_COMPUTER_PEER_REQUIREMENT",
            requirement("app.iywclaw"),
        ),
    ] {
        if std::env::var_os(name).is_none() {
            println!("cargo:rustc-env={name}={value}");
        }
    }
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).expect("Computer Use source directory unavailable") {
        let path = entry.expect("Computer Use source entry unavailable").path();
        if path.is_dir() {
            files.extend(source_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}
