use serde_json::json;

use super::{read_servers_at, remove_server_at, upsert_server_at};

#[test]
fn config_toml_round_trips_and_preserves_unrelated_sections() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config.toml");
    std::fs::write(&path, "[cli]\nauto_update = true\n\n[ui]\nyolo = false\n")
        .expect("seed config");

    let stdio = json!({
        "type": "stdio",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        "env": {"TOKEN": "sk-abc"},
        "cwd": "/work/dir",
    });
    let http = json!({
        "type": "http",
        "url": "https://mcp.example.com/mcp",
        "headers": {"Authorization": "Bearer xyz"},
    });
    let sse = json!({"type": "sse", "url": "https://mcp.example.com/sse"});
    upsert_server_at(&path, "fs", &stdio).expect("stdio");
    upsert_server_at(&path, "remote", &http).expect("http");
    upsert_server_at(&path, "events", &sse).expect("sse");

    let servers = read_servers_at(&path).expect("read back");
    assert_eq!(servers.len(), 3);
    assert_eq!(servers["fs"]["cwd"], "/work/dir");
    assert_eq!(servers["remote"]["headers"]["Authorization"], "Bearer xyz");
    assert_eq!(servers["events"]["type"], "sse");
    assert_native_shape(&path);

    assert!(remove_server_at(&path, "fs").expect("remove"));
    assert!(!remove_server_at(&path, "missing").expect("remove missing"));
    let remaining = read_servers_at(&path).expect("remaining");
    assert_eq!(remaining.len(), 2);
}

fn assert_native_shape(path: &std::path::Path) {
    let raw = std::fs::read_to_string(path).expect("raw config");
    let root: toml::Value = raw.parse().expect("TOML");
    let table = root.as_table().expect("root table");
    assert!(table.contains_key("cli"));
    assert!(table.contains_key("ui"));
    let servers = table["mcp_servers"].as_table().expect("servers");
    let stdio = servers["fs"].as_table().expect("stdio");
    assert!(!stdio.contains_key("type"));
    assert_eq!(stdio["cwd"].as_str(), Some("/work/dir"));
    assert_eq!(servers["events"]["type"].as_str(), Some("sse"));
}
