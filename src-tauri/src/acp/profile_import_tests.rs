use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::registry;
use crate::models::agent::AgentType;

use super::{
    import_entry, import_existing_profiles, import_profile_specs, profile_import_specs,
    ProfileImportSpec, ProfileSourceRoots,
};

fn source_roots(base: &Path) -> ProfileSourceRoots {
    ProfileSourceRoots::new(
        base.join("home"),
        base.join("xdg-config"),
        base.join("xdg-data"),
    )
}

fn has_entry(spec: &ProfileImportSpec, source: &Path, destination: &Path) -> bool {
    spec.entries.iter().any(|entry| {
        entry.source_root.join(&entry.source_relative) == source
            && entry.destination_relative == destination
    })
}

fn find_spec(specs: &[ProfileImportSpec], agent_type: AgentType) -> &ProfileImportSpec {
    specs
        .iter()
        .find(|spec| spec.agent_type == agent_type)
        .expect("agent import spec")
}

#[test]
fn profile_import_specs_cover_all_private_profiles() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let sources = source_roots(temp.path());

    let specs = profile_import_specs(&paths, &sources);

    assert_eq!(specs.len(), registry::all_acp_agents().len());
    for agent_type in registry::all_acp_agents() {
        assert_eq!(
            find_spec(&specs, agent_type).destination_root,
            paths.profile(agent_type).root
        );
    }
}

#[test]
fn profile_import_specs_map_special_profile_layouts() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let sources = source_roots(temp.path());
    let specs = profile_import_specs(&paths, &sources);

    let claude = find_spec(&specs, AgentType::ClaudeCode);
    assert!(has_entry(
        claude,
        &sources.home.join(".claude.json"),
        Path::new(".claude.json")
    ));
    let opencode = find_spec(&specs, AgentType::OpenCode);
    assert!(has_entry(
        opencode,
        &sources.xdg_config.join("opencode/opencode.json"),
        Path::new("config/opencode/opencode.json")
    ));
    assert!(has_entry(
        opencode,
        &sources.xdg_data.join("opencode/auth.json"),
        Path::new("data/opencode/auth.json")
    ));
}

#[test]
fn profile_import_specs_honor_resolved_source_profiles() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let custom_codex = temp.path().join("custom-codex-home");
    let sources = source_roots(temp.path()).with_profile(AgentType::Codex, custom_codex.clone());

    let specs = profile_import_specs(&paths, &sources);

    assert!(has_entry(
        find_spec(&specs, AgentType::Codex),
        &custom_codex.join("config.toml"),
        Path::new("config.toml")
    ));
}

#[test]
fn import_copies_allowlisted_files_and_destination_wins() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let sources = source_roots(temp.path());
    seed_codex_source(&sources.home);
    seed_claude_source(&sources.home);
    let codex = paths.profile(AgentType::Codex).root;
    fs::create_dir_all(&codex).expect("codex destination");
    fs::write(codex.join("config.toml"), "destination = true\n").expect("seed destination");
    fs::create_dir_all(codex.join("skills/demo")).expect("destination skill");
    fs::write(codex.join("skills/demo/SKILL.md"), "# Destination\n")
        .expect("seed destination skill");

    let report = import_existing_profiles(&paths, &sources).expect("import profiles");

    assert!(report.imported_files >= 4);
    assert_eq!(
        fs::read_to_string(codex.join("config.toml")).expect("read destination"),
        "destination = true\n"
    );
    assert!(codex.join("auth.json").is_file());
    assert_eq!(
        fs::read_to_string(codex.join("skills/demo/SKILL.md")).expect("read skill"),
        "# Destination\n"
    );
    assert!(codex.join("skills/demo/reference.md").is_file());
    assert!(!codex.join("skills/demo/cache").exists());
    assert!(!codex.join("skills/demo/session").exists());
    assert!(!codex.join("skills/demo/install.lock").exists());
    assert!(!codex.join("sessions").exists());
    assert!(!codex.join("unknown.txt").exists());
}

#[test]
fn import_keeps_claude_and_codex_sources_unchanged() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let sources = source_roots(temp.path());
    seed_codex_source(&sources.home);
    seed_claude_source(&sources.home);
    let before = tree_snapshot(&sources.home);

    import_existing_profiles(&paths, &sources).expect("import profiles");

    assert_eq!(tree_snapshot(&sources.home), before);
    let claude = paths.profile(AgentType::ClaudeCode).root;
    assert!(claude.join("settings.json").is_file());
    assert!(claude.join(".claude.json").is_file());
    assert!(paths
        .profile(AgentType::Codex)
        .root
        .join("auth.json")
        .is_file());
}

#[test]
fn preparation_failure_does_not_activate_earlier_profiles() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let source = temp.path().join("source");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("config.toml"), "source = true\n").expect("source file");
    let codex = paths.profile(AgentType::Codex).root;
    fs::create_dir_all(&codex).expect("codex destination");
    fs::write(codex.join("config.toml"), "destination = true\n").expect("codex file");
    let claude = paths.profile(AgentType::ClaudeCode).root;
    fs::create_dir_all(claude.parent().expect("claude parent")).expect("profile parent");
    fs::write(&claude, "not a directory\n").expect("invalid later destination");
    let specs = [
        single_file_spec(AgentType::Codex, codex.clone(), &source, "config.toml"),
        single_file_spec(AgentType::ClaudeCode, claude, &source, "config.toml"),
    ];

    assert!(import_profile_specs(&paths, &specs).is_err());
    assert_eq!(
        fs::read_to_string(codex.join("config.toml")).expect("read codex"),
        "destination = true\n"
    );
    assert!(staging_is_empty(&paths.staging_dir()));
}

#[test]
fn unsafe_entry_fails_without_touching_destination_or_staging() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let destination = paths.profile(AgentType::Codex).root;
    fs::create_dir_all(&destination).expect("destination");
    fs::write(destination.join("config.toml"), "keep = true\n").expect("seed destination");
    let source = temp.path().join("source");
    fs::create_dir_all(&source).expect("source");
    let spec = ProfileImportSpec {
        agent_type: AgentType::Codex,
        destination_root: destination.clone(),
        entries: vec![import_entry(&source, "../outside", "config.toml")],
    };

    assert!(import_profile_specs(&paths, &[spec]).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("config.toml")).expect("read destination"),
        "keep = true\n"
    );
    assert!(!paths.staging_dir().exists() || staging_is_empty(&paths.staging_dir()));
}

#[cfg(windows)]
#[test]
fn import_skips_directory_links_that_escape_the_profile() {
    use std::os::windows::fs::symlink_dir;

    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let source = temp.path().join("source");
    let external = temp.path().join("external-skill");
    fs::create_dir_all(source.join("skills")).expect("source skills");
    fs::create_dir_all(&external).expect("external skill");
    fs::write(external.join("SKILL.md"), "# External\n").expect("external file");
    symlink_dir(&external, source.join("skills/agently-mail")).expect("directory link");
    fs::write(source.join("settings.json"), "{}\n").expect("normal source file");
    let destination = paths.profile(AgentType::ClaudeCode).root;
    let spec = ProfileImportSpec {
        agent_type: AgentType::ClaudeCode,
        destination_root: destination.clone(),
        entries: vec![
            import_entry(&source, "settings.json", "settings.json"),
            import_entry(&source, "skills", "skills"),
        ],
    };

    let report = import_profile_specs(&paths, &[spec]).expect("import skips link");

    assert_eq!(report.imported_files, 1);
    assert_eq!(report.skipped_unsafe_links, 1);
    assert!(destination.join("settings.json").is_file());
    assert!(!destination.join("skills/agently-mail").exists());
}

fn seed_codex_source(home: &Path) {
    let root = home.join(".codex");
    fs::create_dir_all(root.join("skills/demo")).expect("codex source");
    fs::create_dir_all(root.join("sessions")).expect("codex sessions");
    fs::write(root.join("config.toml"), "source = true\n").expect("codex config");
    fs::write(root.join("auth.json"), "{\"token\":\"secret\"}\n").expect("codex auth");
    fs::write(root.join("skills/demo/SKILL.md"), "# Demo\n").expect("codex skill");
    fs::write(root.join("skills/demo/reference.md"), "reference\n").expect("skill reference");
    fs::create_dir_all(root.join("skills/demo/cache")).expect("nested cache");
    fs::create_dir_all(root.join("skills/demo/session")).expect("nested session");
    fs::write(root.join("skills/demo/cache/data"), "cache\n").expect("cache data");
    fs::write(root.join("skills/demo/session/data"), "session\n").expect("session data");
    fs::write(root.join("skills/demo/install.lock"), "lock\n").expect("lock file");
    fs::write(root.join("sessions/session.jsonl"), "session\n").expect("codex session");
    fs::write(root.join("unknown.txt"), "unknown\n").expect("unknown file");
}

fn seed_claude_source(home: &Path) {
    let root = home.join(".claude");
    fs::create_dir_all(root.join("cache")).expect("claude source");
    fs::write(root.join("settings.json"), "{\"permissions\":{}}\n").expect("claude settings");
    fs::write(root.join("cache/state.json"), "cache\n").expect("claude cache");
    fs::write(home.join(".claude.json"), "{\"mcpServers\":{}}\n").expect("claude mcp");
}

fn single_file_spec(
    agent_type: AgentType,
    destination_root: PathBuf,
    source: &Path,
    name: &str,
) -> ProfileImportSpec {
    ProfileImportSpec {
        agent_type,
        destination_root,
        entries: vec![import_entry(source, name, name)],
    }
}

fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut snapshot = Vec::new();
    collect_tree_snapshot(root, root, &mut snapshot);
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    snapshot
}

fn collect_tree_snapshot(
    root: &Path,
    current: &Path,
    snapshot: &mut Vec<(PathBuf, Vec<u8>, std::time::SystemTime)>,
) {
    for entry in fs::read_dir(current).expect("read source tree") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect_tree_snapshot(root, &path, snapshot);
        } else {
            let metadata = fs::metadata(&path).expect("source metadata");
            snapshot.push((
                path.strip_prefix(root)
                    .expect("relative source")
                    .to_path_buf(),
                fs::read(&path).expect("read source"),
                metadata.modified().expect("source modified time"),
            ));
        }
    }
}

fn staging_is_empty(path: &Path) -> bool {
    fs::read_dir(path).expect("read staging").next().is_none()
}
